import { describe, expect, it } from "vitest";
import type { ChatStreamEvent, Message, StreamEvent, ToolApprovalRequest } from "@/bindings";
import { liveText, liveThinking, reduceStream, resetStreams, useStreams } from "./streams";

const ev = (event: StreamEvent, conversationId = "c1"): ChatStreamEvent => ({
  conversationId,
  event,
});
const empty = { live: {}, failures: {} };
const started = ev({
  type: "started",
  messageId: "r1",
  parentId: "u1",
  model: "anthropic/x",
  startedAt: 1,
});

describe("reduceStream", () => {
  it("builds a live reply from deltas and times the thinking", () => {
    let s = reduceStream(empty, started, 100);
    s = reduceStream(s, ev({ type: "thinkingDelta", text: "Let me " }), 200);
    s = reduceStream(s, ev({ type: "thinkingDelta", text: "think" }), 300);
    s = reduceStream(s, ev({ type: "textDelta", text: "Hello" }), 2_300);
    s = reduceStream(s, ev({ type: "textDelta", text: " there" }), 2_400);
    const live = s.live["c1"];
    expect(live && liveText(live)).toBe("Hello there");
    expect(live && liveThinking(live)).toBe("Let me think");
    expect(live?.blocks[0]).toEqual({
      kind: "thinking",
      text: "Let me think",
      startedAt: 200,
      endedAt: 2_300,
    });
  });

  it("follows a tool call from start to result, in order with the text", () => {
    let s = reduceStream(empty, started, 1);
    s = reduceStream(s, ev({ type: "textDelta", text: "Looking." }), 2);
    s = reduceStream(s, ev({ type: "toolUse", id: "t", name: "fs__read" }), 3);
    s = reduceStream(
      s,
      ev({
        type: "toolCall",
        id: "t",
        name: "fs__read",
        server: "fs",
        tool: "read",
        title: "Read file",
        inputJson: '{"path":"a"}',
      }),
      4,
    );
    const request = { toolUseId: "t", conversationId: "c1" } as ToolApprovalRequest;
    s = reduceStream(s, ev({ type: "toolApproval", request }), 5);
    const tool = () => {
      const b = s.live["c1"]?.blocks[1];
      return b?.kind === "tool" ? b.call : null;
    };
    expect(tool()?.status).toBe("awaitingApproval");
    expect(tool()?.approval).toBe(request);
    s = reduceStream(s, ev({ type: "toolRunning", id: "t" }), 6);
    expect(tool()?.status).toBe("running");
    s = reduceStream(
      s,
      ev({
        type: "toolResult",
        id: "t",
        content: [{ type: "text", text: "data" }],
        isError: false,
        durationMs: 12,
      }),
      7,
    );
    expect(tool()).toMatchObject({ status: "done", server: "fs", tool: "read", durationMs: 12 });
    s = reduceStream(s, ev({ type: "textDelta", text: "Done." }), 8);
    expect(s.live["c1"]?.blocks.map((b) => b.kind)).toEqual(["text", "tool", "text"]);
  });

  it("ignores deltas for chats without a started reply", () => {
    const s = reduceStream(empty, ev({ type: "textDelta", text: "x" }), 1);
    expect(s).toBe(empty);
  });

  it("clears the live reply when done, and records failures until the next start", () => {
    let s = reduceStream(empty, started, 1);
    s = reduceStream(s, ev({ type: "done", message: null }), 2);
    expect(s.live).toEqual({});
    s = reduceStream(s, started, 3);
    const partial = { id: "r1" } as Message;
    s = reduceStream(s, ev({ type: "failed", error: "boom", message: partial }), 4);
    expect(s.live).toEqual({});
    expect(s.failures["c1"]).toEqual({ error: "boom", message: partial });
    s = reduceStream(s, started, 5);
    expect(s.failures).toEqual({});
  });

  it("keeps chats apart", () => {
    let s = reduceStream(empty, started, 1);
    s = reduceStream(s, ev({ type: "textDelta", text: "other" }, "c2"), 2);
    expect(s.live["c1"]?.blocks).toEqual([]);
    expect(s.live["c2"]).toBeUndefined();
  });

  it("closes an approval card as soon as it is answered", () => {
    resetStreams();
    const { apply, markAnswered } = useStreams.getState();
    apply(started, 1);
    apply(ev({ type: "toolUse", id: "t", name: "time__now" }), 2);
    const request = { toolUseId: "t", conversationId: "c1" } as ToolApprovalRequest;
    apply(ev({ type: "toolApproval", request }), 3);
    markAnswered("c1", "t", true);
    const b = useStreams.getState().live["c1"]?.blocks[0];
    expect(b?.kind === "tool" && b.call.status).toBe("approved");
    expect(b?.kind === "tool" && b.call.approval).toBe(null);
  });

  it("closes every waiting card of a tool on allow-always", () => {
    resetStreams();
    const { apply, markToolAllowed } = useStreams.getState();
    apply(started, 1);
    for (const id of ["a", "b", "c"]) {
      apply(ev({ type: "toolUse", id, name: id === "c" ? "fs__read" : "fs__write" }), 2);
      apply(
        ev({
          type: "toolCall",
          id,
          name: "x",
          server: "fs",
          tool: id === "c" ? "read" : "write",
          title: null,
          inputJson: "{}",
        }),
        3,
      );
      const request = { toolUseId: id, conversationId: "c1" } as ToolApprovalRequest;
      apply(ev({ type: "toolApproval", request }), 4);
    }
    markToolAllowed("c1", "fs", "write");
    const statuses = useStreams
      .getState()
      .live["c1"]?.blocks.map((b) => (b.kind === "tool" ? b.call.status : null));
    expect(statuses).toEqual(["approved", "approved", "awaitingApproval"]);
  });
});
