import { describe, expect, it } from "vitest";
import type { ChatStreamEvent, Message, StreamEvent } from "@/bindings";
import { reduceStream } from "./streams";

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
    s = reduceStream(s, ev({ type: "toolUse", id: "t", name: "search" }), 2_500);
    const live = s.live["c1"];
    expect(live?.text).toBe("Hello there");
    expect(live?.thinking).toBe("Let me think");
    expect(live?.thinkingStartedAt).toBe(200);
    expect(live?.thinkingEndedAt).toBe(2_300);
    expect(live?.tools).toEqual([{ id: "t", name: "search" }]);
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
    expect(s.live["c1"]?.text).toBe("");
    expect(s.live["c2"]).toBeUndefined();
  });
});
