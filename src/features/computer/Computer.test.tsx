import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { QueryClientProvider } from "@tanstack/react-query";
import { describe, expect, it } from "vitest";
import type { ComputerApproval, ToolApprovalRequest } from "@/bindings";
import type { ToolCallView } from "@/features/chat/streams";
import { callHeadline, toolKind, usageRows } from "@/features/tools/toolView";
import { grantLabel } from "@/features/workspace/grants";
import { createQueryClient } from "@/lib/api";
import {
  askApproval,
  emitStream,
  fakeState,
  heldReplyId,
  resetFakeBackend,
} from "@/test/fakeBackend";
import {
  fakeComputerState,
  setFakeComputerActivity,
  setFakeComputerStatus,
} from "@/test/fakeComputer";
import { renderApp } from "@/test/renderApp";
import { ComputerBar } from "./ComputerBar";
import { computerKeys } from "./queries";

const ON = { settings: { computer: { enabled: true } } };

function view(inputJson: string, status: ToolCallView["status"] = "done"): ToolCallView {
  return {
    id: "x",
    name: "computer",
    tool: "computer",
    server: "computer",
    title: "Use the computer",
    inputJson,
    status,
    approval: null,
    result: null,
    isError: false,
    durationMs: null,
    provider: false,
  } as ToolCallView;
}

describe("Settings → Computer use", () => {
  it("switches computer use on, picks a screen and lists the apps every chat may use", async () => {
    renderApp("/settings/computer", {
      backend: {
        settings: {
          computer: { alwaysAllowedApps: [{ id: "notepad.exe", name: "Notepad" }] },
        },
      },
    });
    const toggle = await screen.findByRole("switch", { name: "Let models use the computer" });
    expect(screen.getByText(/Ctrl\+Alt\+Esc/)).toBeInTheDocument();
    fireEvent.click(toggle);
    await waitFor(() => expect(fakeState().settings.computer.enabled).toBe(true));

    const pick = screen.getByRole("combobox", { name: "Screen" });
    await within(pick).findByRole("option", { name: "LG HDR (1920×1080)" });
    fireEvent.change(pick, { target: { value: "\\\\.\\DISPLAY2" } });
    await waitFor(() => expect(fakeState().settings.computer.display).toBe("\\\\.\\DISPLAY2"));

    fireEvent.click(screen.getByRole("switch", { name: "Minimize Oxide while a chat acts" }));
    await waitFor(() => expect(fakeState().settings.computer.hideOxide).toBe(false));

    const apps = screen.getByRole("region", { name: "Apps" });
    expect(apps).toHaveTextContent("Notepad");
    fireEvent.click(within(apps).getByRole("button", { name: "Remove" }));
    await waitFor(() => expect(fakeState().settings.computer.alwaysAllowedApps).toEqual([]));
    expect(await within(apps).findByText("None yet.")).toBeInTheDocument();
  });

  it("shows a chat controlling the computer and stops it", async () => {
    renderApp("/settings/computer", { backend: ON });
    await screen.findByRole("switch", { name: "Let models use the computer" });
    act(() =>
      setFakeComputerActivity({ conversationId: "c2", state: "working", action: "Clicking" }),
    );
    const busy = await screen.findByText("A chat is controlling the computer.");
    fireEvent.click(
      within(busy.parentElement as HTMLElement).getByRole("button", { name: "Stop" }),
    );
    await waitFor(() => expect(fakeComputerState().stops).toBe(1));
    await waitFor(() =>
      expect(screen.queryByText("A chat is controlling the computer.")).not.toBeInTheDocument(),
    );
  });

  it("says when the panic shortcut couldn't be registered, and when this system can't", async () => {
    const { client } = renderApp("/settings/computer", { backend: ON });
    await screen.findByRole("switch", { name: "Let models use the computer" });
    setFakeComputerStatus({ panicError: "Ctrl+Alt+Esc couldn't be registered." });
    await act(() => client.invalidateQueries({ queryKey: computerKeys.status }));
    expect(await screen.findByText("Ctrl+Alt+Esc couldn't be registered.")).toBeInTheDocument();
    setFakeComputerStatus({ supported: false });
    await act(() => client.invalidateQueries({ queryKey: computerKeys.status }));
    expect(await screen.findByText(/Computer use works on Windows for now/)).toBeInTheDocument();
  });
});

describe("Settings → Models: can a model see images", () => {
  it("marks a model as seeing images, and resets it", async () => {
    renderApp("/settings/models");
    const list = await screen.findByRole("region", { name: "LM Studio models" });
    const toggle = within(list).getByRole("button", { name: "qwen/qwen3-14b can see images" });
    expect(toggle).toHaveTextContent("No images");
    expect(toggle).toHaveAttribute("aria-pressed", "false");
    fireEvent.click(toggle);
    await waitFor(() =>
      expect(fakeState().settings.models.images).toEqual([
        { model: "lmstudio/qwen/qwen3-14b", images: true },
      ]),
    );
    await waitFor(() =>
      expect(
        within(list).getByRole("button", { name: "qwen/qwen3-14b can see images" }),
      ).toHaveTextContent("Images (set)"),
    );
    fireEvent.click(
      within(list).getByRole("button", { name: "Reset whether qwen/qwen3-14b can see images" }),
    );
    await waitFor(() => expect(fakeState().settings.models.images).toEqual([]));
  });
});

describe("Computer use in a reply", () => {
  async function startReply() {
    renderApp("/chat/c2", { backend: { replies: "hold", ...ON } });
    await screen.findByText(/warm neutral palette/);
    const box = screen.getByRole("textbox", { name: "Message" });
    fireEvent.change(box, { target: { value: "Open Notepad and write hello" } });
    fireEvent.keyDown(box, { key: "Enter" });
    const live = await screen.findByLabelText("Reply in progress");
    await within(live).findByText("Partial");
    return live;
  }

  function ask(id: string, inputJson: string, computer: ComputerApproval) {
    const request: ToolApprovalRequest = {
      conversationId: "c2",
      messageId: heldReplyId("c2"),
      toolUseId: id,
      server: "computer",
      tool: "computer",
      title: "Use the computer",
      description: "Use the user's computer like a person.",
      inputJson,
      readOnly: false,
      destructive: false,
      skill: null,
      computer,
      requestedAt: Date.now(),
    };
    act(() => {
      emitStream("c2", { type: "toolUse", id, name: "computer" });
      emitStream("c2", {
        type: "toolCall",
        id,
        name: "computer",
        server: "computer",
        tool: "computer",
        title: "Use the computer",
        inputJson,
      });
      askApproval(request);
    });
  }

  it("asks once for the chat (no Allow once), then for each app", async () => {
    const live = await startReply();
    ask("u1", JSON.stringify({ action: "screenshot" }), {
      kind: "session",
      app: null,
      display: "Dell U2720Q (2560×1440)",
      action: "Taking a screenshot",
      panicShortcut: "Ctrl+Alt+Esc",
    });
    const card = await screen.findByRole("group", {
      name: "Allow Oxide to control your computer",
    });
    expect(card).toHaveTextContent("Let Oxide control your computer in this chat?");
    expect(card).toHaveTextContent("Next: Taking a screenshot · on Dell U2720Q (2560×1440)");
    expect(card).toHaveTextContent("never types into password or payment fields");
    expect(card).toHaveTextContent("Ctrl+Alt+Esc");
    expect(within(card).queryByRole("button", { name: "Allow once" })).not.toBeInTheDocument();
    fireEvent.click(within(card).getByRole("button", { name: "Allow for this chat" }));
    await waitFor(() =>
      expect(fakeState().answers).toContainEqual({
        conversationId: "c2",
        toolUseId: "u1",
        decision: "allowChat",
      }),
    );
    act(() => emitStream("c2", { type: "toolRunning", id: "u1" }));
    expect(
      await within(live).findByRole("button", { name: /Taking a screenshot/ }),
    ).toBeInTheDocument();

    ask("u2", JSON.stringify({ action: "left_click", coordinate: [683, 384] }), {
      kind: "app",
      app: { id: "notepad.exe", name: "Notepad" },
      display: "Dell U2720Q (2560×1440)",
      action: "Clicking at (683, 384)",
      panicShortcut: "Ctrl+Alt+Esc",
    });
    const appCard = await screen.findByRole("group", { name: "Allow Oxide to use Notepad" });
    expect(appCard).toHaveTextContent("Next: Clicking at (683, 384)");
    fireEvent.click(
      within(appCard).getByRole("checkbox", { name: "Always allow Notepad in every chat" }),
    );
    fireEvent.click(within(appCard).getByRole("button", { name: "Allow once" }));
    await waitFor(() =>
      expect(fakeState().settings.computer.alwaysAllowedApps).toEqual([
        { id: "notepad.exe", name: "Notepad" },
      ]),
    );
    expect(fakeState().answers).toContainEqual({
      conversationId: "c2",
      toolUseId: "u2",
      decision: "allowOnce",
    });
  });

  it("names computer actions, rows and grants", () => {
    const click = view(JSON.stringify({ action: "left_click", coordinate: [683, 384] }));
    expect(toolKind(click)).toBe("computer");
    expect(callHeadline(click)).toEqual({ action: "Clicked", detail: "at (683, 384)" });
    expect(
      callHeadline(view(JSON.stringify({ action: "type", text: "hello\nworld" }), "running")),
    ).toEqual({ action: "Typing", detail: "“hello world”" });
    expect(callHeadline(view(JSON.stringify({ action: "key", text: "ctrl+s" })))).toEqual({
      action: "Pressed",
      detail: "ctrl+s",
    });
    expect(
      callHeadline(
        view(JSON.stringify({ action: "scroll", scroll_direction: "up", coordinate: [5, 6] })),
      ),
    ).toEqual({ action: "Scrolled", detail: "up at (5, 6)" });
    expect(callHeadline(view(JSON.stringify({ action: "fly" })))).toEqual({
      action: "Used the computer",
      detail: "",
    });
    const rows = usageRows([
      {
        id: "1",
        conversationId: "c2",
        messageId: "m",
        toolUseId: "u1",
        kind: "computer",
        server: "computer",
        tool: "computer",
        label: "Clicking at (683, 384)",
        isError: false,
        createdAt: 1,
      },
    ]);
    expect(rows[0]).toMatchObject({ title: "Computer", details: ["Clicking at (683, 384)"] });
    expect(grantLabel("builtin/computer")).toBe("Control the computer");
    expect(grantLabel("computer-app/notepad.exe")).toBe("Computer: notepad.exe");
  });
});

describe("The control bar", () => {
  it("shows what a chat does and stops every chat", async () => {
    resetFakeBackend();
    render(
      <QueryClientProvider client={createQueryClient()}>
        <ComputerBar />
      </QueryClientProvider>,
    );
    const bar = screen.getByRole("status", { name: "Oxide is controlling your computer" });
    // The bar has read the (empty) activity before anything changes.
    await act(() => new Promise((r) => setTimeout(r, 0)));
    act(() =>
      setFakeComputerActivity({
        conversationId: "c2",
        state: "working",
        action: "Typing “hello”",
      }),
    );
    expect(await within(bar).findByText("· Typing “hello”")).toBeInTheDocument();
    act(() => setFakeComputerActivity({ conversationId: "c2", state: "waiting", action: null }));
    expect(await within(bar).findByText("· Waiting for you in Oxide")).toBeInTheDocument();
    fireEvent.click(within(bar).getByRole("button", { name: "Stop" }));
    await waitFor(() => expect(fakeComputerState().stops).toBe(1));
  });
});
