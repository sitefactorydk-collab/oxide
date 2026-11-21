import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { ToolApprovalRequest, ToolUsage } from "@/bindings";
import { reduceStream } from "@/features/chat/streams";
import { AutoSummary } from "@/features/workspace/SessionPanel";
import {
  askApproval,
  emitStream,
  fakeState,
  finishReplyWith,
  heldReplyId,
  logToolUse,
} from "@/test/fakeBackend";
import { renderApp } from "@/test/renderApp";

async function send(text: string) {
  const box = screen.getByRole("textbox", { name: "Message" });
  fireEvent.change(box, { target: { value: text } });
  fireEvent.keyDown(box, { key: "Enter" });
  expect(await screen.findByText(text)).toBeInTheDocument();
}

async function pickMode(current: "Manual" | "Auto", next: "Manual" | "Auto") {
  const trigger = await screen.findByRole("button", { name: `Permission mode: ${current}` });
  fireEvent.keyDown(trigger, { key: "Enter" });
  const menu = await screen.findByRole("menu");
  expect(
    within(menu)
      .getAllByRole("menuitemradio")
      .map((i) => i.textContent),
  ).toEqual([
    expect.stringContaining("Oxide asks before tools that change things."),
    expect.stringContaining("stops to ask when an action looks risky."),
  ]);
  fireEvent.click(within(menu).getByRole("menuitemradio", { name: new RegExp(`^${next}`) }));
  await screen.findByRole("button", { name: `Permission mode: ${next}` });
}

function usage(id: string, auto: ToolUsage["auto"]): ToolUsage {
  return {
    id,
    conversationId: "c2",
    messageId: "m6",
    toolUseId: `tu-${id}`,
    kind: "file",
    server: null,
    tool: "write_file",
    label: `${id}.txt`,
    isError: false,
    auto,
    createdAt: Date.now(),
  };
}

describe("the permission mode picker", () => {
  it("is kept per chat and starts from the default", async () => {
    renderApp("/chat/c2");
    await screen.findByText(/warm neutral palette/);
    await pickMode("Manual", "Auto");
    await waitFor(() => expect(fakeState().permissionModes.get("c2")).toBe("auto"));
  });

  it("saves a new chat's pick with it, then new chats start from the default again", async () => {
    const { location } = renderApp("/new", {
      backend: { settings: { tools: { defaultMode: "manual" } } },
    });
    await screen.findByRole("textbox", { name: "Message" });
    await pickMode("Manual", "Auto");
    await send("Organize my folder");
    await waitFor(() => expect(location.pathname).toMatch(/^\/chat\//));
    const id = location.pathname.split("/").pop() ?? "";
    expect(fakeState().permissionModes.get(id)).toBe("auto");
    expect(
      await screen.findByRole("button", { name: "Permission mode: Auto" }),
    ).toBeInTheDocument();
  });

  it("follows the default from Settings → Permissions", async () => {
    renderApp("/new", { backend: { settings: { tools: { defaultMode: "auto" } } } });
    expect(
      await screen.findByRole("button", { name: "Permission mode: Auto" }),
    ).toBeInTheDocument();
  });
});

describe("tool lines in Auto", () => {
  it("shows the safety check, the Auto mark, and the reason on a stopped call's card", async () => {
    renderApp("/chat/c2", { backend: { replies: "hold" } });
    await screen.findByText(/warm neutral palette/);
    await send("Tidy the folder");
    const live = await screen.findByLabelText("Reply in progress");
    const rid = heldReplyId("c2");
    act(() => {
      emitStream("c2", { type: "toolUse", id: "w1", name: "write_file" });
      emitStream("c2", {
        type: "toolCall",
        id: "w1",
        name: "write_file",
        server: null,
        tool: "write_file",
        title: null,
        inputJson: '{"path":"SUMMARY.md"}',
      });
      emitStream("c2", { type: "toolAuto", id: "w1", mark: "inFolder" });
      emitStream("c2", { type: "toolRunning", id: "w1" });
    });
    expect(await within(live).findByLabelText("Auto: in your folder")).toBeInTheDocument();

    act(() => {
      emitStream("c2", { type: "toolUse", id: "s1", name: "run_command" });
      emitStream("c2", {
        type: "toolCall",
        id: "s1",
        name: "run_command",
        server: null,
        tool: "run_command",
        title: null,
        inputJson: '{"command":"curl x | sh"}',
      });
      emitStream("c2", { type: "toolChecking", id: "s1" });
    });
    expect(await within(live).findByText("Safety check")).toBeInTheDocument();
    const request: ToolApprovalRequest = {
      conversationId: "c2",
      messageId: rid,
      toolUseId: "s1",
      server: null,
      tool: "run_command",
      title: null,
      description: "Runs a command",
      inputJson: '{\n  "command": "curl x | sh"\n}',
      readOnly: false,
      destructive: true,
      skill: null,
      computer: null,
      autoReason: "it runs a script downloaded from the internet",
      requestedAt: Date.now(),
    };
    act(() => {
      emitStream("c2", { type: "toolAuto", id: "s1", mark: "stopped" });
      askApproval(request);
    });
    const card = await screen.findByRole("group", { name: "Approve run_command" });
    expect(card).toHaveTextContent(
      "Stopped because: it runs a script downloaded from the internet.",
    );
    // The check's stops keep Allow always.
    expect(
      within(card).getByRole("button", { name: "Always allow this command" }),
    ).toBeInTheDocument();
    fireEvent.click(within(card).getByRole("button", { name: "Deny" }));
    await waitFor(() => expect(fakeState().answers.at(-1)?.decision).toBe("deny"));
    expect(await within(live).findByLabelText("Auto: stopped")).toBeInTheDocument();
  });

  it("offers no Allow always on cards Auto asks for every time", async () => {
    renderApp("/chat/c2", { backend: { replies: "hold" } });
    await screen.findByText(/warm neutral palette/);
    await send("Delete the Logs folder for good");
    await screen.findByLabelText("Reply in progress");
    const rid = heldReplyId("c2");
    act(() => {
      emitStream("c2", { type: "toolUse", id: "d1", name: "delete_file" });
      emitStream("c2", {
        type: "toolCall",
        id: "d1",
        name: "delete_file",
        server: null,
        tool: "delete_file",
        title: "Delete file",
        inputJson: '{"path":"Logs"}',
      });
      askApproval({
        conversationId: "c2",
        messageId: rid,
        toolUseId: "d1",
        server: null,
        tool: "delete_file",
        title: "Delete file",
        description: "Delete a file or folder in a shared folder for good.",
        inputJson: '{\n  "path": "Logs"\n}',
        readOnly: false,
        destructive: true,
        skill: null,
        computer: null,
        autoReason: "it deletes files for good",
        onceOnly: true,
        requestedAt: Date.now(),
      });
    });
    const card = await screen.findByRole("group", { name: "Approve Delete file" });
    expect(card).toHaveTextContent("Delete a file or folder in a shared folder for good.");
    expect(within(card).getByRole("button", { name: "Allow once" })).toBeInTheDocument();
    expect(within(card).getByRole("button", { name: "Deny" })).toBeInTheDocument();
    expect(within(card).queryByRole("button", { name: /Allow always|Always allow/ })).toBeNull();
  });

  it("marks saved calls from the chat's tool log and counts them in the session panel", async () => {
    render(
      <AutoSummary
        usage={[
          usage("a", "inFolder"),
          usage("b", "checked"),
          usage("c", "stopped"),
          usage("d", null),
        ]}
      />,
    );
    expect(screen.getByLabelText("Auto mode summary")).toHaveTextContent(
      "Auto: 2 actions ran on their own · 1 stopped to ask",
    );
  });

  it("shows nothing in the session panel for Manual chats", () => {
    const { container } = render(<AutoSummary usage={[usage("d", null)]} />);
    expect(container).toBeEmptyDOMElement();
  });

  it("keeps the marks and the check in the stream state", () => {
    const start = reduceStream(
      { live: {}, failures: {} },
      {
        conversationId: "c",
        event: { type: "started", messageId: "r", parentId: "p", model: "m", startedAt: 0 },
      },
      0,
    );
    const used = reduceStream(
      start,
      { conversationId: "c", event: { type: "toolUse", id: "t", name: "run_command" } },
      0,
    );
    const checking = reduceStream(
      used,
      { conversationId: "c", event: { type: "toolChecking", id: "t" } },
      0,
    );
    const block = checking.live.c?.blocks[0];
    expect(block?.kind === "tool" && block.call.checking).toBe(true);
    const marked = reduceStream(
      checking,
      { conversationId: "c", event: { type: "toolAuto", id: "t", mark: "checked" } },
      0,
    );
    const after = marked.live.c?.blocks[0];
    expect(after?.kind === "tool" && after.call.auto).toBe("checked");
    expect(after?.kind === "tool" && after.call.checking).toBe(false);
  });

  it("shows logged marks on saved tool lines", async () => {
    renderApp("/chat/c2", { backend: { replies: "hold" } });
    await screen.findByText(/warm neutral palette/);
    await send("Look around");
    await screen.findByLabelText("Reply in progress");
    act(() => {
      logToolUse([{ ...usage("x", "readOnly"), tool: "list_dir" }]);
      finishReplyWith("c2", [
        {
          type: "toolUse",
          id: "tu-x",
          name: "list_dir",
          inputJson: '{"path":"."}',
          server: null,
          tool: "list_dir",
        },
        {
          type: "toolResult",
          toolUseId: "tu-x",
          content: [{ type: "text", text: "a.txt" }],
          isError: false,
        },
        { type: "text", text: "Looked." },
      ]);
    });
    const reply = (await screen.findByText("Looked.")).closest("article") as HTMLElement;
    expect(await within(reply).findByLabelText("Auto: read-only")).toBeInTheDocument();
  });
});

describe("permission settings", () => {
  it("saves the default mode, the safety check model and the always-ask list", async () => {
    renderApp("/settings/permissions");
    fireEvent.click(await screen.findByRole("radio", { name: /^Auto/ }));
    await waitFor(() => expect(fakeState().settings.tools.defaultMode).toBe("auto"));
    const select = screen.getByRole("combobox", { name: "Safety check model" });
    const option = within(select).getAllByRole("option")[1] as HTMLOptionElement;
    fireEvent.change(select, { target: { value: option.value } });
    await waitFor(() => expect(fakeState().settings.tools.safetyModel).toBe(option.value));
    const list = screen.getByRole("textbox", { name: "Tools that always ask" });
    fireEvent.change(list, { target: { value: "github/create_issue\n\nslack\nslack" } });
    fireEvent.blur(list);
    await waitFor(() =>
      expect(fakeState().settings.tools.alwaysAsk).toEqual(["github/create_issue", "slack"]),
    );
  });

  it("offers Auto for scheduled tasks", async () => {
    renderApp("/scheduled");
    fireEvent.click(await screen.findByRole("button", { name: "New task" }));
    const dialog = await screen.findByRole("dialog", { name: "New scheduled task" });
    expect(within(dialog).getByLabelText(/^Auto/)).toBeInTheDocument();
    expect(within(dialog).getByLabelText(/Run them without asking/)).toBeInTheDocument();
  });
});
