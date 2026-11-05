import { describe, expect, it } from "vitest";
import type { Message } from "@/bindings";
import { activeThread, messageText, newestLeafUnder, siblingsOf, thinkingOf } from "./thread";

function msg(id: string, parentId: string | null, text = id): Message {
  return {
    id,
    conversationId: "c",
    parentId,
    role: "user",
    content: [{ type: "text", text }],
    model: null,
    stopReason: null,
    usage: null,
    feedback: null,
    createdAt: 0,
  };
}

describe("activeThread", () => {
  it("follows parents from the newest message", () => {
    // a → b → c, then b edited to b2 (sibling of b), then d under b2.
    const all = [msg("a", null), msg("b", "a"), msg("c", "b"), msg("b2", "a"), msg("d", "b2")];
    expect(activeThread(all).map((m) => m.id)).toEqual(["a", "b2", "d"]);
  });

  it("handles empty lists and roots that were edited", () => {
    expect(activeThread([])).toEqual([]);
    expect(activeThread([msg("a", null), msg("a2", null)]).map((m) => m.id)).toEqual(["a2"]);
  });
});

describe("message helpers", () => {
  it("joins text blocks and reads thinking", () => {
    const m: Pick<Message, "content"> = {
      content: [
        { type: "thinking", text: "hmm", signature: null, durationMs: 6200 },
        { type: "text", text: "One" },
        { type: "text", text: "Two" },
      ],
    };
    expect(messageText(m)).toBe("One\n\nTwo");
    expect(thinkingOf(m)).toEqual({ seconds: 6, text: "hmm" });
    expect(thinkingOf({ content: [] })).toBeUndefined();
  });

  it("finds versions of a message and the newest leaf of a branch", () => {
    // a → b → c; b edited to b2 → d; c has a regenerated sibling c2.
    const all = [
      msg("a", null),
      msg("b", "a"),
      msg("c", "b"),
      msg("b2", "a"),
      msg("d", "b2"),
      msg("c2", "b"),
    ];
    const b = all[1]!;
    expect(siblingsOf(all, b).map((m) => m.id)).toEqual(["b", "b2"]);
    expect(newestLeafUnder(all, "b")).toBe("c2");
    expect(newestLeafUnder(all, "b2")).toBe("d");
    expect(newestLeafUnder(all, "d")).toBe("d");
    expect(activeThread(all, newestLeafUnder(all, "b")).map((m) => m.id)).toEqual(["a", "b", "c2"]);
  });
});

describe("activeThread with a leaf", () => {
  it("ends at the given message, hiding replies being replaced", () => {
    const m = (id: string, parentId: string | null): Message => ({
      id,
      conversationId: "c",
      parentId,
      role: "user",
      content: [],
      model: null,
      createdAt: 0,
      stopReason: null,
      usage: null,
      feedback: null,
    });
    const all = [m("a", null), m("b", "a"), m("c", "b")];
    expect(activeThread(all, "b").map((x) => x.id)).toEqual(["a", "b"]);
    expect(activeThread(all, "missing").map((x) => x.id)).toEqual(["a", "b", "c"]);
  });
});
