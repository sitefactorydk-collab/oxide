import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { ToolApprovalRequest } from "@/bindings";
import {
  askApproval,
  emitStream,
  fakeState,
  finishReplyWith,
  heldReplyId,
  logToolUse,
} from "@/test/fakeBackend";
import { fakeMcpState } from "@/test/fakeMcp";
import { renderApp } from "@/test/renderApp";
import { callHeadline, replySegments, usageRows } from "./toolView";

async function send(text: string) {
  const box = screen.getByRole("textbox", { name: "Message" });
  fireEvent.change(box, { target: { value: text } });
  fireEvent.keyDown(box, { key: "Enter" });
  expect(await screen.findByText(text)).toBeInTheDocument();
}

function request(messageId: string, toolUseId = "t1"): ToolApprovalRequest {
  return {
    conversationId: "c2",
    messageId,
    toolUseId,
    server: "fs",
    tool: "write_file",
    title: "Write file",
    description: "[fs] Writes a file",
    inputJson: '{\n  "path": "C:/notes.txt"\n}',
    readOnly: false,
    destructive: true,
    requestedAt: Date.now(),
  };
}

describe("tool calls in a reply", () => {
  it("asks for approval inline, then shows the call running and its result", async () => {
    renderApp("/chat/c2", { backend: { replies: "hold" } });
    await screen.findByText(/warm neutral palette/);
    await send("Save my notes");
    const live = await screen.findByLabelText("Reply in progress");
    await within(live).findByText("Partial");
    const rid = heldReplyId("c2");

    act(() => {
      emitStream("c2", { type: "toolUse", id: "t1", name: "fs__write_file" });
      emitStream("c2", {
        type: "toolCall",
        id: "t1",
        name: "fs__write_file",
        server: "fs",
        tool: "write_file",
        title: "Write file",
        inputJson: '{"path":"C:/notes.txt"}',
      });
      askApproval(request(rid));
    });
    const card = await screen.findByRole("group", { name: "Approve Write file" });
    expect(card).toHaveTextContent("Allow Oxide to use Write file from fs?");
    expect(card).toHaveTextContent("can change or delete data");
    expect(within(card).getByLabelText("Arguments")).toHaveTextContent('"path": "C:/notes.txt"');
    fireEvent.click(within(card).getByRole("button", { name: "Allow once" }));
    await waitFor(() =>
      expect(fakeState().answers).toEqual([
        { conversationId: "c2", toolUseId: "t1", decision: "allowOnce" },
      ]),
    );

    // The card closes as soon as it's answered, before the call runs.
    expect(screen.queryByRole("group", { name: "Approve Write file" })).not.toBeInTheDocument();
    expect(within(live).getByText("Allowed · starting")).toBeInTheDocument();
    act(() => emitStream("c2", { type: "toolRunning", id: "t1" }));
    expect(await within(live).findByText("Running")).toBeInTheDocument();
    act(() => {
      emitStream("c2", {
        type: "toolResult",
        id: "t1",
        content: [{ type: "text", text: "Wrote 12 bytes" }],
        isError: false,
        durationMs: 40,
      });
      emitStream("c2", { type: "textDelta", text: "Saved them." });
    });
    expect(await within(live).findByText("Saved them.")).toBeInTheDocument();
    // Working again shows only while nothing streams; the text is last now.
    expect(within(live).queryByText("Working")).not.toBeInTheDocument();

    act(() => {
      finishReplyWith("c2", [
        { type: "text", text: "Partial" },
        {
          type: "toolUse",
          id: "t1",
          name: "fs__write_file",
          inputJson: '{"path":"C:/notes.txt"}',
          server: "fs",
          tool: "write_file",
        },
        {
          type: "toolResult",
          toolUseId: "t1",
          content: [{ type: "text", text: "Wrote 12 bytes" }],
          isError: false,
        },
        { type: "text", text: "Saved them." },
      ]);
    });
    const reply = (await screen.findByText("Saved them.")).closest("article") as HTMLElement;
    const toggle = within(reply).getByRole("button", { name: /write_file/ });
    expect(toggle).toHaveTextContent("C:/notes.txt");
    fireEvent.click(toggle);
    expect(within(reply).getByText("Wrote 12 bytes")).toBeInTheDocument();
  });

  it("denies, and offers waiting approvals the window didn't see live", async () => {
    renderApp("/chat/c2");
    await screen.findByText(/warm neutral palette/);
    act(() => askApproval(request("r-old", "t9")));
    // No live reply in this window: the card comes from pendingToolApprovals.
    fireEvent.focus(window);
    const card = await screen.findByRole("group", { name: "Approve Write file" });
    fireEvent.click(within(card).getByRole("button", { name: "Deny" }));
    await waitFor(() => expect(fakeState().answers.at(-1)?.decision).toBe("deny"));
    await waitFor(() =>
      expect(screen.queryByRole("group", { name: "Approve Write file" })).not.toBeInTheDocument(),
    );
  });

  it("lists what the chat used in the details panel", async () => {
    renderApp("/chat/c2", { ui: { rightPanelOpen: true } });
    await screen.findByText(/warm neutral palette/);
    act(() =>
      logToolUse([
        usage("u1", "webSearch", null, "web_search", "rust palettes"),
        usage("u2", "webSearch", null, "web_search", "oxide colors"),
        usage("u3", "webFetch", null, "web_fetch", "https://example.com/a"),
        usage("u4", "connector", "fs", "read_file", "C:/a.txt"),
      ]),
    );
    emitStream("c2", {
      type: "toolResult",
      id: "x",
      content: [],
      isError: false,
      durationMs: 0,
    });
    const list = await screen.findByRole("list", { name: "Used in this chat" });
    await waitFor(() => expect(within(list).getAllByRole("listitem")).toHaveLength(3));
    expect(list).toHaveTextContent("Web search2rust palettes · oxide colors");
    expect(list).toHaveTextContent("Web pagesexample.com");
    expect(list).toHaveTextContent("fsread_file");
  });

  it("tells the backend the local time zone", async () => {
    renderApp("/new");
    await waitFor(() => expect(fakeState().clock).not.toBeNull());
    expect(fakeState().clock?.offset).toBe(-new Date().getTimezoneOffset());
  });
});

function usage(
  id: string,
  kind: "webSearch" | "webFetch" | "connector",
  server: string | null,
  tool: string,
  label: string,
) {
  return {
    id,
    conversationId: "c2",
    messageId: "m6",
    toolUseId: `tu-${id}`,
    kind,
    server,
    tool,
    label,
    isError: false,
    createdAt: Date.now(),
  };
}

describe("tool helpers", () => {
  it("pairs saved calls with results and summarizes provider searches", () => {
    const segs = replySegments([
      { type: "thinking", text: "plan", signature: null, durationMs: 2400 },
      { type: "text", text: "Looking." },
      { type: "serverToolUse", id: "s1", name: "web_search", inputJson: '{"query":"oxide"}' },
      {
        type: "serverToolResult",
        toolUseId: "s1",
        rawJson: JSON.stringify({ content: [{ title: "Oxide", url: "https://o.example" }] }),
      },
      {
        type: "toolUse",
        id: "t1",
        name: "web_fetch",
        inputJson: '{"url":"https://o.example/docs"}',
        server: null,
        tool: "web_fetch",
      },
      { type: "toolResult", toolUseId: "t1", content: [], isError: true },
      { type: "text", text: "Done." },
    ]);
    expect(segs.map((s) => s.kind)).toEqual(["thinking", "text", "tool", "tool", "text"]);
    const [, , search, fetch] = segs;
    if (search?.kind !== "tool" || fetch?.kind !== "tool") throw new Error("tools expected");
    expect(callHeadline(search.call)).toEqual({ action: "Searched the web", detail: "oxide" });
    expect(search.call.result?.[0]).toEqual({
      type: "text",
      text: "1. Oxide\n   https://o.example",
    });
    expect(callHeadline(fetch.call)).toEqual({ action: "Read", detail: "o.example" });
    expect(fetch.call.status).toBe("error");
  });

  it("collapses repeated uses", () => {
    const rows = usageRows([
      usage("a", "connector", "fs", "read", "x"),
      usage("b", "connector", "fs", "read", "y"),
      usage("c", "connector", "time", "now", "z"),
    ]);
    expect(rows.map((r) => [r.title, r.count, r.details])).toEqual([
      ["fs", 2, ["read"]],
      ["time", 1, ["now"]],
    ]);
  });
});

describe("tool settings", () => {
  it("saves permission switches and the step limit", async () => {
    renderApp("/settings/permissions");
    const toggle = await screen.findByRole("switch", {
      name: "Run read-only connector tools without asking",
    });
    fireEvent.click(toggle);
    await waitFor(() => expect(fakeState().settings.tools.autoAllowReadOnly).toBe(true));
    const steps = screen.getByRole("spinbutton", { name: /Most tool steps per reply/ });
    fireEvent.change(steps, { target: { value: "500" } });
    fireEvent.blur(steps);
    await waitFor(() => expect(fakeState().settings.tools.maxSteps).toBe(200));
  });

  it("picks a search service, stores its key and tests it", async () => {
    renderApp("/settings/web-search");
    fireEvent.click(await screen.findByRole("radio", { name: /Brave Search/ }));
    await waitFor(() => expect(fakeState().settings.webSearch.provider).toBe("brave"));
    fireEvent.click(await screen.findByRole("button", { name: "Test search" }));
    expect(await screen.findByText(/needs an API key/)).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("API key"), {
      target: { value: "BSA-secret-key-1234" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save key" }));
    await waitFor(() => expect(fakeState().searchKeys.brave).toBe("••••1234"));
    expect(screen.getByLabelText("API key")).toHaveAttribute("placeholder", "Saved (••••1234)");
    fireEvent.click(screen.getByRole("button", { name: "Test search" }));
    expect(await screen.findByText(/2 results for a test search/)).toBeInTheDocument();
  });

  it("switches web search off for a chat in the + menu", async () => {
    renderApp("/chat/c2");
    await screen.findByText(/warm neutral palette/);
    const trigger = await screen.findByRole("button", { name: "Add files, context and tools" });
    fireEvent.keyDown(trigger, { key: "Enter" });
    const item = await screen.findByRole("menuitemcheckbox", { name: "Web search" });
    expect(item).toHaveAttribute("aria-checked", "true");
    fireEvent.click(item);
    await waitFor(() =>
      expect(fakeMcpState().chatTools.get("c2")?.disabledBuiltins).toEqual([
        "web_search",
        "web_fetch",
      ]),
    );
  });
});
