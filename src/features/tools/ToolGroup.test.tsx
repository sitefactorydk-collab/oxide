import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { ContentBlock, ToolUsage } from "@/bindings";
import type { ToolCallView } from "@/features/chat/streams";
import { translate, type T } from "@/i18n";
import {
  askApproval,
  emitStream,
  fakeState,
  finishReplyWith,
  heldReplyId,
  logToolUse,
} from "@/test/fakeBackend";
import { renderApp } from "@/test/renderApp";
import {
  failedText,
  firstSentence,
  groupItems,
  latestSentence,
  liveStatus,
  summarizeSteps,
  summaryText,
  type TranscriptItem,
} from "./toolGroups";
import { jumpToToolCall } from "./reveal";

const en: T = (key, vars) => translate("en", key, vars);
const da: T = (key, vars) => translate("da", key, vars);

function call(
  id: string,
  tool: string,
  over: Partial<ToolCallView> = {},
  inputJson = "{}",
): ToolCallView {
  return {
    id,
    name: tool,
    server: null,
    tool,
    title: null,
    inputJson,
    status: "done",
    approval: null,
    result: [{ type: "text", text: "ok" }],
    isError: false,
    durationMs: null,
    provider: false,
    ...over,
  };
}
const tool = (c: ToolCallView): TranscriptItem => ({ kind: "tool", call: c });
const text = (t: string): TranscriptItem => ({ kind: "text", text: t });
const thinking = (t: string): TranscriptItem => ({ kind: "thinking", text: t, seconds: 2 });
const image: ContentBlock = {
  type: "image",
  mediaType: "image/png",
  source: { kind: "base64", data: "AAAA" },
};

describe("grouping a reply's tool calls", () => {
  it("folds consecutive calls and the thinking between them; prose ends a group", () => {
    const entries = groupItems([
      text("Let me look."),
      thinking("First I list it."),
      tool(call("a", "list_dir")),
      thinking("Now read."),
      tool(call("b", "read_file")),
      text("Here is what I found."),
      tool(call("c", "write_file")),
      text("Done."),
    ]);
    expect(entries.map((e) => e.kind)).toEqual(["text", "group", "text", "group", "text"]);
    const first = entries[1];
    expect(first?.kind === "group" && first.key).toBe("a");
    expect(first?.kind === "group" && first.steps.map((s) => s.kind)).toEqual([
      "thinking",
      "tool",
      "thinking",
      "tool",
    ]);
  });

  it("keeps thinking that leads straight to prose on its own", () => {
    const entries = groupItems([thinking("Hmm."), text("Answer.")]);
    expect(entries.map((e) => e.kind)).toEqual(["thinking", "text"]);
  });

  it("keeps artifacts, media, computer use, the browser's pictures and charts outside", () => {
    const entries = groupItems([
      tool(call("a", "list_dir")),
      tool(call("art", "artifact")),
      tool(call("b", "read_file")),
      tool(call("img", "generate_image", { result: [image] })),
      tool(call("pc", "computer", { server: "computer" })),
      tool(call("br", "navigate", { server: "browser", status: "running", result: null })),
      tool(call("shot", "screenshot", { server: "browser", result: [image] })),
      tool(call("py", "run_python", { result: [image] })),
      tool(call("py2", "run_python")),
      tool(call("bad-art", "artifact", { status: "error", isError: true })),
    ]);
    expect(entries.map((e) => (e.kind === "group" ? `group:${e.key}` : `${e.kind}`))).toEqual([
      "group:a",
      "outside",
      "group:b",
      "outside",
      "outside",
      "outside",
      "outside",
      "outside",
      "group:py2",
    ]);
    // A browser step that finished without a picture folds.
    const done = groupItems([tool(call("br", "navigate", { server: "browser" }))]);
    expect(done[0]?.kind).toBe("group");
  });

  it("splits a group at a compaction", () => {
    const entries = groupItems([
      tool(call("a", "list_dir")),
      {
        kind: "compaction",
        compaction: {
          id: "x",
          conversationId: "c",
          summary: "s",
          coversThroughMessageId: null,
          afterToolUseId: "a",
          createdAt: 1,
        } as never,
      },
      tool(call("b", "list_dir")),
    ]);
    expect(entries.map((e) => e.kind)).toEqual(["group", "compaction", "group"]);
  });
});

describe("the summary line", () => {
  const steps = (calls: ToolCallView[]) =>
    groupItems(calls.map(tool)).flatMap((e) => (e.kind === "group" ? e.steps : []));

  it("names kinds and counts in plain words, in order of first use", () => {
    const s = summarizeSteps(
      steps([
        call("1", "list_dir"),
        call("2", "run_command"),
        call("3", "list_dir"),
        call("4", "run_command"),
        call("5", "list_dir"),
        call("6", "run_command"),
        call("7", "list_dir"),
        call("8", "write_file"),
      ]),
    );
    expect(summaryText(s, en)).toBe("Listed a folder 4 times, ran 3 commands, wrote a file");
    expect(failedText(s, en)).toBeNull();
  });

  it("counts failures and connectors by name, singular and plural", () => {
    const s = summarizeSteps(
      steps([
        call("1", "web_search"),
        call("2", "web_fetch"),
        call("3", "web_fetch", { status: "error", isError: true }),
        call("4", "create_issue", { server: "github" }),
        call("5", "read_file"),
        call("6", "read_file"),
      ]),
    );
    expect(summaryText(s, en)).toBe(
      "Searched the web, read 2 web pages, used github, read 2 files",
    );
    expect(failedText(s, en)).toBe("1 failed");
  });

  it("has Danish words", () => {
    const s = summarizeSteps(
      steps([
        call("1", "list_dir"),
        call("2", "list_dir"),
        call("3", "run_command", { status: "error", isError: true }),
        call("4", "write_file"),
      ]),
    );
    expect(summaryText(s, da)).toBe("Listede en mappe 2 gange, kørte en kommando, skrev en fil");
    expect(failedText(s, da)).toBe("1 mislykkedes");
  });

  it("words a running group's status line", () => {
    const thinkingNow = {
      kind: "thinking" as const,
      text: "Checking the names. Then the",
      seconds: 0,
      liveSince: 1,
    };
    const done = call("1", "write_file");
    const run = call("2", "run_command", { status: "running", result: null }, '{"command":"ls"}');
    expect(
      liveStatus(
        [
          { kind: "tool", call: done },
          { kind: "tool", call: run },
        ],
        en,
      ),
    ).toEqual({
      label: "Running ls",
      thinking: false,
    });
    expect(
      liveStatus([{ kind: "tool", call: { ...run, status: "ready", checking: true } }], en).label,
    ).toBe("Safety check");
    expect(liveStatus([{ kind: "tool", call: done }, thinkingNow], en)).toEqual({
      label: "Checking the names.",
      thinking: true,
    });
    expect(liveStatus([{ kind: "tool", call: done }], da).label).toBe("Skrev en fil");
    // Open: the rows show the running step, so the line says what is done.
    expect(
      liveStatus(
        [
          { kind: "tool", call: done },
          { kind: "tool", call: run },
        ],
        en,
        true,
      ).label,
    ).toBe("Wrote a file");
    expect(latestSentence("No sentence yet")).toBe("");
  });

  it("shortens thinking to its first sentence", () => {
    expect(firstSentence("I should list the folder.  Then move things.")).toBe(
      "I should list the folder.",
    );
    expect(firstSentence("no full stop here")).toBe("no full stop here");
    expect(firstSentence("x".repeat(200))).toHaveLength(121);
  });
});

async function send(textValue: string) {
  const box = screen.getByRole("textbox", { name: "Message" });
  fireEvent.change(box, { target: { value: textValue } });
  fireEvent.keyDown(box, { key: "Enter" });
  expect(await screen.findByText(textValue)).toBeInTheDocument();
}

function emitCall(id: string, toolName: string, inputJson: string) {
  emitStream("c2", { type: "toolUse", id, name: toolName });
  emitStream("c2", {
    type: "toolCall",
    id,
    name: toolName,
    server: null,
    tool: toolName,
    title: null,
    inputJson,
  });
}

function emitDone(id: string, out: string, isError = false) {
  emitStream("c2", {
    type: "toolResult",
    id,
    content: [{ type: "text", text: out }],
    isError,
    durationMs: 10,
  });
}

const savedReply: ContentBlock[] = [
  {
    type: "thinking",
    text: "I will list the folder first. Then sort it.",
    signature: null,
    durationMs: 2000,
  },
  {
    type: "toolUse",
    id: "l1",
    name: "list_dir",
    inputJson: '{"path":"C:/Downloads"}',
    server: null,
    tool: "list_dir",
  },
  {
    type: "toolResult",
    toolUseId: "l1",
    content: [{ type: "text", text: "a.pdf" }],
    isError: false,
  },
  {
    type: "toolUse",
    id: "r1",
    name: "run_command",
    inputJson: '{"command":"mkdir PDFs"}',
    server: null,
    tool: "run_command",
  },
  {
    type: "toolResult",
    toolUseId: "r1",
    content: [{ type: "text", text: "Access denied" }],
    isError: true,
  },
  {
    type: "toolUse",
    id: "w1",
    name: "write_file",
    inputJson: '{"path":"SUMMARY.md"}',
    server: null,
    tool: "write_file",
  },
  {
    type: "toolResult",
    toolUseId: "w1",
    content: [{ type: "text", text: "Wrote 1 KB" }],
    isError: false,
  },
  { type: "text", text: "All sorted." },
];

function usage(toolUseId: string, mark: ToolUsage["auto"]): ToolUsage {
  return {
    id: toolUseId,
    conversationId: "c2",
    messageId: "m",
    toolUseId,
    kind: "file",
    server: null,
    tool: "list_dir",
    label: null,
    isError: false,
    createdAt: 1,
    auto: mark,
  } as ToolUsage;
}

describe("tool groups in the transcript", () => {
  it("shows a running group as a status line that follows the work, then folds it", async () => {
    renderApp("/chat/c2", {
      backend: { replies: "hold", settings: { toolSteps: "collapsed" } },
    });
    await screen.findByText(/warm neutral palette/);
    await send("Organize my downloads");
    const live = await screen.findByLabelText("Reply in progress");
    await within(live).findByText("Partial");

    act(() => {
      emitStream("c2", { type: "thinkingDelta", text: "Let me list it first. Then sort." });
      emitCall("l1", "list_dir", '{"path":"C:/Downloads"}');
      emitStream("c2", { type: "toolAuto", id: "l1", mark: "readOnly" });
      emitStream("c2", { type: "toolRunning", id: "l1" });
    });
    // One line: the running step and the reply's time; no rows, no separate "Working".
    const group = await within(live).findByRole("group", { name: "Tool steps" });
    const line = within(group).getByRole("button", { name: "Listing C:/Downloads" });
    expect(line).toHaveAttribute("aria-expanded", "false");
    expect(line).toHaveTextContent(/\d+s/);
    expect(within(live).queryByRole("status")).not.toBeInTheDocument();

    // A thought being written: its newest finished sentence.
    act(() => {
      emitDone("l1", "a.pdf");
      emitStream("c2", {
        type: "thinkingDelta",
        text: "Three PDFs there. I will make a folder for",
      });
    });
    expect(
      await within(group).findByRole("button", { name: "Three PDFs there." }),
    ).toBeInTheDocument();

    act(() => {
      emitCall("r1", "run_command", '{"command":"mkdir PDFs"}');
      emitStream("c2", { type: "toolRunning", id: "r1" });
    });
    const running = await within(group).findByRole("button", { name: "Running mkdir PDFs" });

    // Opened: the rows, with the running one last; the line says what is done.
    fireEvent.click(running);
    const header = within(group).getByRole("button", { name: "Listed a folder" });
    expect(header).toHaveAttribute("aria-expanded", "true");
    const row = within(group).getByRole("button", { name: /^Listed C:\/Downloads/ });
    expect(within(row).getByLabelText("Auto: read-only")).toBeInTheDocument();
    expect(within(group).getByRole("button", { name: /^Running mkdir PDFs/ })).toHaveTextContent(
      "Running",
    );
    expect(within(group).getByText("Three PDFs there.")).toBeInTheDocument();
    fireEvent.click(header);

    act(() => {
      emitDone("r1", "Access denied", true);
      emitStream("c2", { type: "textDelta", text: "Sorted." });
    });
    // Prose started: the group folds to its summary.
    const folded = await within(live).findByRole("button", {
      name: "Listed a folder, ran a command · 1 failed",
    });
    expect(folded).toHaveAttribute("aria-expanded", "false");
    expect(within(live).queryByText("mkdir PDFs")).not.toBeInTheDocument();
    expect(within(live).getByText("· 1 failed")).toBeInTheDocument();
  });

  it("folds a saved reply into one line; opened, rows show marks, failures and details", async () => {
    renderApp("/chat/c2", {
      backend: { replies: "hold", settings: { toolSteps: "collapsed" } },
    });
    await screen.findByText(/warm neutral palette/);
    await send("Organize my downloads");
    await screen.findByLabelText("Reply in progress");
    act(() => {
      logToolUse([usage("l1", "readOnly"), usage("w1", "inFolder")]);
      finishReplyWith("c2", savedReply);
    });
    const reply = (await screen.findByText("All sorted.")).closest("article") as HTMLElement;
    const line = within(reply).getByRole("button", {
      name: "Listed a folder, ran a command, wrote a file · 1 failed",
    });
    expect(line).toHaveAttribute("aria-expanded", "false");
    expect(within(reply).queryByText("SUMMARY.md")).not.toBeInTheDocument();
    // The thinking went into the group too.
    expect(within(reply).queryByRole("button", { name: /Thought for/ })).not.toBeInTheDocument();

    fireEvent.click(line);
    expect(line).toHaveAttribute("aria-expanded", "true");
    expect(within(reply).getByText("I will list the folder first.")).toBeInTheDocument();
    const failed = within(reply).getByRole("button", { name: /Ran/ });
    expect(failed).toHaveTextContent("mkdir PDFs");
    expect(failed).toHaveTextContent("Failed");
    expect(await within(reply).findByLabelText("Auto: read-only")).toBeInTheDocument();
    expect(within(reply).getByLabelText("Auto: in your folder")).toBeInTheDocument();
    fireEvent.click(failed);
    expect(within(reply).getByText("Access denied")).toBeInTheDocument();

    fireEvent.click(line);
    expect(within(reply).queryByText("mkdir PDFs")).not.toBeInTheDocument();

    // Jump to the call (Used in this session) opens the group again.
    act(() => jumpToToolCall("w1"));
    await waitFor(() => expect(line).toHaveAttribute("aria-expanded", "true"));
    expect(reply.querySelector('[data-tool-call="w1"]')).not.toBeNull();
  });

  it("keeps a group with a waiting approval open, with the card in place", async () => {
    renderApp("/chat/c2", {
      backend: { replies: "hold", settings: { toolSteps: "collapsed" } },
    });
    await screen.findByText(/warm neutral palette/);
    await send("Delete the logs");
    const live = await screen.findByLabelText("Reply in progress");
    await within(live).findByText("Partial");
    const rid = heldReplyId("c2");
    act(() => {
      emitCall("l1", "list_dir", '{"path":"logs"}');
      emitDone("l1", "x.log");
      emitStream("c2", { type: "textDelta", text: "Now deleting." });
      emitCall("d1", "delete_file", '{"path":"logs/x.log"}');
      askApproval({
        conversationId: "c2",
        messageId: rid,
        toolUseId: "d1",
        server: null,
        tool: "delete_file",
        title: null,
        description: null,
        inputJson: '{"path":"logs/x.log"}',
        readOnly: false,
        destructive: true,
        skill: null,
        computer: null,
        requestedAt: Date.now(),
      });
    });
    const card = await within(live).findByRole("group", { name: "Approve delete_file" });
    const group = card.closest('[role="group"][aria-label="Tool steps"]') as HTMLElement;
    const header = within(group).getAllByRole("button")[0] as HTMLElement;
    expect(header).toHaveAttribute("aria-expanded", "true");
    fireEvent.click(header);
    expect(header).toHaveAttribute("aria-expanded", "true");
    fireEvent.click(within(card).getByRole("button", { name: "Deny" }));
    await waitFor(() => expect(fakeState().answers.at(-1)?.decision).toBe("deny"));
  });

  it("Settings → Appearance → Show tool steps: expanded keeps groups open", async () => {
    renderApp("/settings/appearance", { backend: { settings: { toolSteps: "collapsed" } } });
    const radio = await screen.findByRole("radio", { name: "Expanded" });
    expect(screen.getByRole("radio", { name: "Collapsed" })).toHaveAttribute(
      "aria-checked",
      "true",
    );
    fireEvent.click(radio);
    await waitFor(() => expect(fakeState().settings.appearance.toolSteps).toBe("expanded"));
  });

  it("opens saved groups when the setting is expanded", async () => {
    renderApp("/chat/c2", { backend: { replies: "hold", settings: { toolSteps: "expanded" } } });
    await screen.findByText(/warm neutral palette/);
    await send("Organize my downloads");
    await screen.findByLabelText("Reply in progress");
    act(() => {
      finishReplyWith("c2", savedReply);
    });
    const reply = (await screen.findByText("All sorted.")).closest("article") as HTMLElement;
    const line = await within(reply).findByRole("button", {
      name: "Listed a folder, ran a command, wrote a file · 1 failed",
    });
    await waitFor(() => expect(line).toHaveAttribute("aria-expanded", "true"));
    expect(within(reply).getByText("SUMMARY.md")).toBeInTheDocument();
  });
});
