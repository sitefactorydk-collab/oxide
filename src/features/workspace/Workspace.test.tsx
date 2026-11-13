import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { TaskItem, ToolApprovalRequest } from "@/bindings";
import { callHeadline, usageRows } from "@/features/tools/toolView";
import {
  askApproval,
  emitStream,
  fakeState,
  heldReplyId,
  logToolUse,
  setTaskList,
} from "@/test/fakeBackend";
import { setPickedPaths } from "@/test/fakeExtensions";
import { renderApp } from "@/test/renderApp";
import { grantLabel } from "./grants";
import { useSessionPanelStore } from "./store";
import { stepLabel, taskProgress } from "./tasks";

const steps = (...s: Array<[string, TaskItem["status"]]>): TaskItem[] =>
  s.map(([title, status]) => ({ title, status }));

async function openPlusMenu() {
  const trigger = await screen.findByRole("button", { name: "Add files, context and tools" });
  fireEvent.keyDown(trigger, { key: "Enter" });
  return screen.findByRole("menu");
}

describe("shared folders", () => {
  it("are picked before a chat exists and granted with it", async () => {
    const { location } = renderApp("/new");
    setPickedPaths("folder", ["C:\\Work\\Photos\\"]);
    const menu = await openPlusMenu();
    fireEvent.click(within(menu).getByRole("menuitem", { name: /Add folder/ }));
    const chips = await screen.findByRole("list", { name: "Shared folders" });
    expect(chips).toHaveTextContent("Photos");

    const box = screen.getByRole("textbox", { name: "Message" });
    fireEvent.change(box, { target: { value: "Organize this folder by file type" } });
    fireEvent.keyDown(box, { key: "Enter" });
    await waitFor(() => expect(location.pathname).toMatch(/^\/chat\//));
    const id = location.pathname.split("/").pop() ?? "";
    expect(
      fakeState()
        .workspaces.get(id)
        ?.folders.map((f) => f.path),
    ).toEqual(["C:\\Work\\Photos"]);
    // The new chat's composer shows the folder from the chat itself; the draft is gone.
    const chatChips = await screen.findByRole("list", { name: "Shared folders" });
    expect(chatChips).toHaveTextContent("Photos");
  });

  it("can be removed from the draft and refuse relative paths", async () => {
    renderApp("/new");
    setPickedPaths("folder", ["/home/me/docs"]);
    fireEvent.click(within(await openPlusMenu()).getByRole("menuitem", { name: /Add folder/ }));
    fireEvent.click(await screen.findByRole("button", { name: "Stop sharing docs" }));
    expect(screen.queryByRole("list", { name: "Shared folders" })).not.toBeInTheDocument();
    setPickedPaths("folder", ["relative/path"]);
    fireEvent.click(within(await openPlusMenu()).getByRole("menuitem", { name: /Add folder/ }));
    expect(await screen.findByText(/isn't a full path/)).toBeInTheDocument();
  });

  it("are managed in the session panel", async () => {
    renderApp("/chat/c2", { ui: { rightPanelOpen: true } });
    const panel = await screen.findByRole("complementary", { name: "Chat details" });
    const folders = within(panel).getByRole("region", { name: "Folders" });
    expect(folders).toHaveTextContent("Share a folder");
    setPickedPaths("folder", ["C:\\Work"]);
    fireEvent.click(within(folders).getByRole("button", { name: "Add folder" }));
    const list = await within(folders).findByRole("list", { name: "Shared folders" });
    expect(list).toHaveTextContent("WorkC:\\Work");

    fireEvent.click(within(list).getByRole("button", { name: "Open Work" }));
    await waitFor(() => expect(fakeState().opened).toContain("C:\\Work"));

    const forGood = within(folders).getByRole("checkbox", { name: /Delete for good/ });
    expect(forGood).not.toBeChecked();
    fireEvent.click(forGood);
    await waitFor(() => expect(fakeState().workspaces.get("c2")?.allowDelete).toBe(true));
    await waitFor(() => expect(forGood).toBeChecked());

    fireEvent.click(within(list).getByRole("button", { name: "Stop sharing Work" }));
    await waitFor(() => expect(fakeState().workspaces.get("c2")?.folders).toEqual([]));
    expect(await within(folders).findByText(/Share a folder/)).toBeInTheDocument();
  });
});

describe("progress", () => {
  it("shows the task list live: the panel opens once, and the step pill reopens it", async () => {
    renderApp("/chat/c2");
    await screen.findByText(/warm neutral palette/);
    expect(screen.queryByRole("complementary", { name: "Chat details" })).not.toBeInTheDocument();
    act(() =>
      setTaskList(
        "c2",
        steps(
          ["Look at the files", "done"],
          ["Sort by type", "inProgress"],
          ["Summarize", "pending"],
        ),
      ),
    );
    // The first task list opens the panel (the user hadn't chosen for this chat).
    const panel = await screen.findByRole("complementary", { name: "Chat details" });
    const progress = within(panel).getByRole("region", { name: "Progress" });
    expect(progress).toHaveTextContent("1 of 3 done");
    const items = within(progress).getAllByRole("listitem");
    expect(items).toHaveLength(3);
    expect(items[1]).toHaveAttribute("aria-current", "step");
    expect(within(items[0] as HTMLElement).getByLabelText("Done")).toBeInTheDocument();

    act(() =>
      setTaskList(
        "c2",
        steps(["Look at the files", "done"], ["Sort by type", "done"], ["Summarize", "done"]),
      ),
    );
    expect(
      await screen.findByRole("button", { name: "3 of 3 done. Show progress" }),
    ).toBeInTheDocument();
    expect(progress).toHaveTextContent("3 of 3 done");
    // Closed, it stays closed for this chat when the list changes; the pill reopens it.
    fireEvent.click(within(panel).getByRole("button", { name: "Close details" }));
    expect(useSessionPanelStore.getState().open.c2).toBe(false);
    act(() => setTaskList("c2", steps(["Look at the files", "done"], ["Check", "pending"])));
    const pill = await screen.findByRole("button", { name: "Step 2 of 2. Show progress" });
    expect(screen.queryByRole("complementary", { name: "Chat details" })).not.toBeInTheDocument();
    fireEvent.click(pill);
    expect(await screen.findByRole("complementary", { name: "Chat details" })).toBeInTheDocument();
    expect(useSessionPanelStore.getState().open.c2).toBe(true);
  });

  it("lists files and commands in Used in this session", async () => {
    renderApp("/chat/c2", { ui: { rightPanelOpen: true } });
    await screen.findByText(/warm neutral palette/);
    const base = { conversationId: "c2", messageId: "m6", isError: false, createdAt: 1 };
    act(() =>
      logToolUse([
        {
          ...base,
          id: "u1",
          toolUseId: "t1",
          kind: "file",
          server: null,
          tool: "list_dir",
          label: ".",
        },
        {
          ...base,
          id: "u2",
          toolUseId: "t2",
          kind: "file",
          server: null,
          tool: "move_file",
          label: "a.png → Images/a.png",
        },
        {
          ...base,
          id: "u3",
          toolUseId: "t3",
          kind: "tasks",
          server: null,
          tool: "task_list",
          label: "Sort",
        },
        {
          ...base,
          id: "u4",
          toolUseId: "t4",
          kind: "shell",
          server: null,
          tool: "run_command",
          label: "git status",
        },
      ]),
    );
    emitStream("c2", { type: "toolResult", id: "x", content: [], isError: false, durationMs: 0 });
    const used = await screen.findByRole("list", { name: "Used in this session" });
    await waitFor(() => expect(within(used).getAllByRole("listitem")).toHaveLength(2));
    expect(used).toHaveTextContent("Files2. · a.png → Images/a.png");
    expect(used).toHaveTextContent("Commandsgit status");
  });

  it("shows tools allowed for the chat and takes them back", async () => {
    renderApp("/chat/c2", { ui: { rightPanelOpen: true } });
    await screen.findByText(/warm neutral palette/);
    expect(screen.queryByRole("region", { name: "Allowed in this chat" })).not.toBeInTheDocument();
    // "Always allow" on a waiting card (from pendingToolApprovals: no live reply here).
    fakeState().grants.set("c2", ["builtin/write_file"]);
    act(() => askApproval(command("t9", "git status")));
    fireEvent.focus(window);
    const card = await screen.findByRole("group", { name: "Approve Run command" });
    fireEvent.click(within(card).getByRole("button", { name: "Always allow this command" }));
    const allowed = await screen.findByRole("region", { name: "Allowed in this chat" });
    await waitFor(() => expect(allowed).toHaveTextContent("Command: git status"));
    expect(allowed).toHaveTextContent("write file");
    fireEvent.click(within(allowed).getByRole("button", { name: "Ask again for write file" }));
    await waitFor(() =>
      expect(fakeState().grants.get("c2")).toEqual(["builtin/run_command:git status"]),
    );
  });
});

function command(toolUseId: string, cmd: string, messageId = "m6"): ToolApprovalRequest {
  return {
    conversationId: "c2",
    messageId,
    toolUseId,
    server: null,
    tool: "run_command",
    title: "Run command",
    description: "Run a command",
    inputJson: JSON.stringify({ command: cmd }, null, 2),
    readOnly: false,
    destructive: true,
    skill: null,
    computer: null,
    requestedAt: Date.now(),
  };
}

describe("needs your input", () => {
  it("marks chats waiting for approval in the sidebar", async () => {
    renderApp("/new");
    await screen.findByRole("link", { name: "Design tokens for light and dark" });
    expect(screen.queryByText("Needs input")).not.toBeInTheDocument();
    act(() => askApproval(command("t1", "npm test")));
    expect(await screen.findByText("Needs input")).toBeInTheDocument();
  });

  it("allows one command at a time", async () => {
    renderApp("/chat/c2", { backend: { replies: "hold" } });
    await screen.findByText(/warm neutral palette/);
    const box = screen.getByRole("textbox", { name: "Message" });
    fireEvent.change(box, { target: { value: "Run the tests" } });
    fireEvent.keyDown(box, { key: "Enter" });
    const live = await screen.findByLabelText("Reply in progress");
    await within(live).findByText("Partial");
    const rid = heldReplyId("c2");
    act(() => {
      for (const [id, cmd] of [
        ["s1", "npm test"],
        ["s2", "npm run lint"],
      ] as const) {
        emitStream("c2", { type: "toolUse", id, name: "run_command" });
        emitStream("c2", {
          type: "toolCall",
          id,
          name: "run_command",
          server: null,
          tool: "run_command",
          title: "Run command",
          inputJson: JSON.stringify({ command: cmd }),
        });
        askApproval(command(id, cmd, rid));
      }
    });
    const cards = await within(live).findAllByRole("group", { name: "Approve Run command" });
    expect(cards).toHaveLength(2);
    const first = cards[0] as HTMLElement;
    expect(first).toHaveTextContent("Commands run on your computer");
    fireEvent.click(within(first).getByRole("button", { name: "Always allow this command" }));
    await waitFor(() =>
      expect(within(live).getAllByRole("group", { name: "Approve Run command" })).toHaveLength(1),
    );
    expect(fakeState().answers.at(-1)).toEqual({
      conversationId: "c2",
      toolUseId: "s1",
      decision: "allowChat",
    });
  });
});

describe("Settings → Permissions → Shell commands", () => {
  it("switches the shell on and saves the allowlist", async () => {
    renderApp("/settings/permissions");
    const section = await screen.findByRole("region", { name: "Shell commands" });
    fireEvent.click(within(section).getByRole("switch", { name: /run commands/ }));
    await waitFor(() => expect(fakeState().settings.shell.enabled).toBe(true));
    const list = await within(section).findByRole("textbox", {
      name: "Commands that run without asking",
    });
    fireEvent.change(list, { target: { value: "git   status\n\nls\nls" } });
    fireEvent.blur(list);
    await waitFor(() => expect(fakeState().settings.shell.allowlist).toEqual(["git status", "ls"]));
  });
});

describe("helpers", () => {
  it("count steps and name the current one", () => {
    expect(stepLabel([])).toBeNull();
    const list = steps(["a", "done"], ["b", "pending"], ["c", "inProgress"]);
    expect(taskProgress(list)).toEqual({ done: 1, total: 3, current: 2 });
    expect(stepLabel(list)).toBe("Step 3 of 3");
    expect(stepLabel(steps(["a", "done"], ["b", "pending"]))).toBe("Step 2 of 2");
    expect(stepLabel(steps(["a", "done"]))).toBe("1 of 1 done");
  });

  it("describe file, task and shell calls", () => {
    const call = (tool: string, inputJson: string, status: "running" | "done" = "done") => ({
      id: "x",
      name: tool,
      server: null,
      tool,
      title: null,
      inputJson,
      status,
      approval: null,
      result: null,
      isError: false,
      durationMs: null,
      provider: false,
    });
    expect(callHeadline(call("move_file", '{"from":"a.png","to":"Images"}'))).toEqual({
      action: "Moved",
      detail: "a.png → Images",
    });
    expect(callHeadline(call("list_dir", "{}", "running"))).toEqual({
      action: "Listing",
      detail: "folder",
    });
    expect(callHeadline(call("read_file", '{"path":"notes.md"}'))).toEqual({
      action: "Read",
      detail: "notes.md",
    });
    expect(
      callHeadline(
        call(
          "task_list",
          '{"tasks":[{"title":"a","status":"done"},{"title":"b","status":"pending"}]}',
        ),
      ),
    ).toEqual({ action: "Updated task list", detail: "1 of 2 done" });
    expect(callHeadline(call("run_command", '{"command":"git status"}'))).toEqual({
      action: "Ran",
      detail: "git status",
    });
    // A connector's tool of the same name stays a connector tool.
    expect(callHeadline({ ...call("read_file", '{"path":"x"}'), server: "fs" }).action).toBe(
      "read_file",
    );
    expect(usageRows([])).toEqual([]);
    expect(grantLabel("fs/read_file")).toBe("read_file · fs");
    expect(grantLabel("builtin/run_command:npm test")).toBe("Command: npm test");
  });
});
