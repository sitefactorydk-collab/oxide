import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { TaskInput } from "@/bindings";
import { emitDesktopRequest } from "@/test/fakeBackend";
import { setPickedPaths } from "@/test/fakeExtensions";
import { fakeRun, fakeScheduleState, seedTask } from "@/test/fakeSchedule";
import { renderApp } from "@/test/renderApp";

function input(patch: Partial<TaskInput> = {}): TaskInput {
  return {
    name: "Morning brief",
    prompt: "Summarize today's news about Rust.",
    schedule: { kind: "cron", expr: "0 9 * * *" },
    model: "anthropic/claude-sonnet-5-5",
    ...patch,
  };
}

describe("Scheduled tasks", () => {
  it("creates a weekday task from the sidebar and opens it", async () => {
    const { location } = renderApp("/new");
    fireEvent.click(await screen.findByRole("link", { name: "Scheduled" }));
    await waitFor(() => expect(location.pathname).toBe("/scheduled"));
    expect(await screen.findByText("No scheduled tasks yet")).toBeInTheDocument();
    // Closing the window keeps Oxide in the tray here, but it doesn't start at sign-in.
    expect(screen.getByText(/Tasks run while Oxide is open or in the tray/)).toHaveTextContent(
      "Oxide doesn't start when you sign in.",
    );

    fireEvent.click(screen.getByRole("button", { name: "New task" }));
    const dialog = await screen.findByRole("dialog", { name: "New scheduled task" });
    fireEvent.change(within(dialog).getByLabelText("Name"), {
      target: { value: "Inbox digest" },
    });
    fireEvent.change(within(dialog).getByLabelText("What should it do?"), {
      target: { value: "Read the shared folder and list what's new." },
    });
    fireEvent.change(within(dialog).getByLabelText("How often"), {
      target: { value: "weekdays" },
    });
    fireEvent.change(within(dialog).getByLabelText("Time"), { target: { value: "07:30" } });
    expect(await within(dialog).findByText(/^Next: /)).toBeInTheDocument();

    setPickedPaths("folder", ["C:\\Users\\me\\Inbox"]);
    fireEvent.click(within(dialog).getByRole("button", { name: "Add folder" }));
    expect(await within(dialog).findByText("C:\\Users\\me\\Inbox")).toBeInTheDocument();
    fireEvent.click(within(dialog).getByLabelText(/Run them without asking/));
    fireEvent.click(within(dialog).getByLabelText("Skip it and wait for the next time"));

    fireEvent.click(within(dialog).getByRole("button", { name: "Create task" }));
    await waitFor(() => expect(location.pathname).toBe("/scheduled/task1"));
    const [saved] = fakeScheduleState().saved;
    expect(saved).toMatchObject({
      name: "Inbox digest",
      schedule: { kind: "cron", expr: "30 7 * * 1-5" },
      folders: ["C:\\Users\\me\\Inbox"],
      approval: "allowAll",
      missed: "skip",
      enabled: true,
    });
    // Saving skills is off for new tasks (it would wait for approval).
    expect(saved?.tools?.disabledBuiltins).toEqual(["save_skill"]);
    expect(saved?.model).toMatch(/^anthropic\//);

    expect(await screen.findByRole("heading", { name: "Inbox digest" })).toBeInTheDocument();
    expect(screen.getByText(/Weekdays at 07:30 · next /)).toBeInTheDocument();
    expect(await screen.findByText("No runs yet.")).toBeInTheDocument();
  });

  it("shows what's wrong with a schedule and refuses to save it", async () => {
    renderApp("/scheduled");
    fireEvent.click(await screen.findByRole("button", { name: "New task" }));
    const dialog = await screen.findByRole("dialog", { name: "New scheduled task" });
    fireEvent.change(within(dialog).getByLabelText("Name"), { target: { value: "Bad" } });
    fireEvent.change(within(dialog).getByLabelText("What should it do?"), {
      target: { value: "x" },
    });
    fireEvent.change(within(dialog).getByLabelText("How often"), { target: { value: "custom" } });
    fireEvent.change(within(dialog).getByLabelText("Cron expression"), {
      target: { value: "every morning" },
    });
    expect(await within(dialog).findByText(/A cron schedule has five parts/)).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole("button", { name: "Create task" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent(
      "A cron schedule has five parts",
    );
    expect(fakeScheduleState().tasks).toHaveLength(0);
  });

  it("runs now, and the history shows each run's output and chat", async () => {
    const { location } = renderApp("/scheduled");
    // The page (and its event listener) is ready before the task appears.
    await screen.findByText("No scheduled tasks yet");
    act(() => void seedTask(input()));
    const row = await screen.findByRole("link", { name: /Morning brief/ });
    expect(row).toHaveTextContent("Every day at 09:00 · Next: ");

    fireEvent.click(screen.getByRole("button", { name: "Run Morning brief now" }));
    await waitFor(() => expect(fakeScheduleState().runNow).toEqual(["task1"]));
    expect(await screen.findByText("Running")).toBeInTheDocument();

    fireEvent.click(row);
    await waitFor(() => expect(location.pathname).toBe("/scheduled/task1"));
    const runs = await screen.findByRole("list", { name: "Runs" });
    expect(await within(runs).findByText("Running")).toBeInTheDocument();
    expect(runs).toHaveTextContent("Run now");

    // The run waits for approval, then finishes with a report.
    const [first] = fakeScheduleState().runs;
    if (!first) throw new Error("Run now made no run");
    act(
      () =>
        void fakeRun("task1", { id: first.id, status: "waitingApproval", conversationId: "c1" }),
    );
    expect(
      await within(runs).findByRole("link", { name: "Open the chat to approve" }),
    ).toHaveAttribute("href", "/chat/c1");
    act(
      () =>
        void fakeRun("task1", {
          id: first.id,
          status: "done",
          finishedAt: Date.now(),
          output: "Three new crates this morning: …",
        }),
    );
    expect(await within(runs).findByText("Three new crates this morning: …")).toBeInTheDocument();
    expect(within(runs).getByText("Done")).toBeInTheDocument();

    // A missed one says why it didn't run.
    act(() => void fakeRun("task1", { status: "missed", finishedAt: Date.now() }));
    expect(await within(runs).findByText(/Oxide wasn't running at this time/)).toBeInTheDocument();

    fireEvent.click(within(runs).getByRole("link", { name: "Open chat" }));
    await waitFor(() => expect(location.pathname).toBe("/chat/c1"));
    // The chat says which task wrote it.
    expect(await screen.findByText(/Scheduled run of “Morning brief”/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("link", { name: "View task" }));
    await waitFor(() => expect(location.pathname).toBe("/scheduled/task1"));
  });

  it("links a run waiting for approval to its chat from the list", async () => {
    const { location } = renderApp("/scheduled");
    await screen.findByText("No scheduled tasks yet");
    act(() => void seedTask(input()));
    await screen.findByRole("link", { name: /Morning brief/ });
    expect(screen.queryByRole("link", { name: "Open the chat to approve" })).toBeNull();
    act(() => void fakeRun("task1", { status: "waitingApproval", conversationId: "c1" }));
    const open = await screen.findByRole("link", { name: "Open the chat to approve" });
    expect(open).toHaveAttribute("href", "/chat/c1");
    fireEvent.click(open);
    await waitFor(() => expect(location.pathname).toBe("/chat/c1"));
  });

  it("drops a run from the history when its chat is deleted", async () => {
    renderApp("/scheduled");
    await screen.findByText("No scheduled tasks yet");
    act(() => {
      seedTask(input());
      fakeRun("task1", { status: "done", conversationId: "c1", output: "Sunny." });
      fakeRun("task1", { status: "done", conversationId: "c2", output: "Rainy." });
    });
    fireEvent.click(await screen.findByRole("link", { name: /Morning brief/ }));
    const runs = await screen.findByRole("list", { name: "Runs" });
    expect(await within(runs).findByText("Sunny.")).toBeInTheDocument();
    expect(within(runs).getByText("Rainy.")).toBeInTheDocument();

    // Delete chat c1 from the sidebar's menu (keyboard: more reliable than pointers in jsdom).
    const sidebar = screen.getByRole("complementary", { name: "Sidebar" });
    const row = within(sidebar).getByRole("link", { name: "Tauri custom title bar on Windows" });
    fireEvent.keyDown(
      within(sidebar).getByRole("button", { name: /More options for Tauri custom title bar/ }),
      { key: "Enter" },
    );
    fireEvent.click(await screen.findByRole("menuitem", { name: "Delete" }));
    await waitFor(() => expect(row).not.toBeInTheDocument());
    await waitFor(() => expect(within(runs).queryByText("Sunny.")).not.toBeInTheDocument());
    expect(within(runs).getByText("Rainy.")).toBeInTheDocument();
    expect(fakeScheduleState().runs.map((r) => r.conversationId)).toEqual(["c2"]);
  });

  it("pauses, resumes, edits and deletes a task", async () => {
    const { location } = renderApp("/scheduled");
    // The page (and its event listener) is ready before the task appears.
    await screen.findByText("No scheduled tasks yet");
    act(() => void seedTask(input()));
    const toggle = await screen.findByRole("switch", { name: "Pause Morning brief" });
    fireEvent.click(toggle);
    await waitFor(() => expect(fakeScheduleState().tasks[0]?.enabled).toBe(false));
    expect(await screen.findByRole("link", { name: /Paused/ })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("switch", { name: "Resume Morning brief" }));
    await waitFor(() => expect(fakeScheduleState().tasks[0]?.enabled).toBe(true));

    fireEvent.click(screen.getByRole("link", { name: /Morning brief/ }));
    await screen.findByRole("heading", { name: "Morning brief" });
    fireEvent.click(screen.getByRole("button", { name: "Edit task" }));
    const dialog = await screen.findByRole("dialog", { name: "Edit task" });
    expect(within(dialog).getByLabelText("How often")).toHaveValue("daily");
    expect(within(dialog).getByLabelText("Time")).toHaveValue("09:00");
    fireEvent.change(within(dialog).getByLabelText("How often"), { target: { value: "weekly" } });
    fireEvent.change(within(dialog).getByLabelText("Day of the week"), {
      target: { value: "5" },
    });
    fireEvent.click(within(dialog).getByRole("button", { name: "Save" }));
    await waitFor(() =>
      expect(fakeScheduleState().tasks[0]?.schedule).toEqual({ kind: "cron", expr: "0 9 * * 5" }),
    );
    expect(await screen.findByText(/Every Friday at 09:00/)).toBeInTheDocument();

    // Radix menus open from the keyboard too, which jsdom handles more reliably than pointers.
    fireEvent.keyDown(screen.getByRole("button", { name: "More" }), { key: "Enter" });
    fireEvent.click(await screen.findByRole("menuitem", { name: "Delete" }));
    const confirm = await screen.findByRole("dialog", { name: "Delete “Morning brief”?" });
    fireEvent.click(within(confirm).getByRole("button", { name: "Delete task" }));
    await waitFor(() => expect(location.pathname).toBe("/scheduled"));
    expect(fakeScheduleState().tasks).toHaveLength(0);
    expect(await screen.findByText("No scheduled tasks yet")).toBeInTheDocument();
  });

  it("opens from the tray or a link", async () => {
    const { location } = renderApp("/new");
    await screen.findByRole("textbox", { name: "Message" });
    act(() => void seedTask(input()));
    act(() => emitDesktopRequest({ type: "openScheduled", taskId: null }));
    await waitFor(() => expect(location.pathname).toBe("/scheduled"));
    act(() => emitDesktopRequest({ type: "openScheduled", taskId: "task1" }));
    await waitFor(() => expect(location.pathname).toBe("/scheduled/task1"));
    expect(await screen.findByRole("heading", { name: "Morning brief" })).toBeInTheDocument();
  });
});
