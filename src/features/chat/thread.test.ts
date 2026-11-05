import { describe, expect, it } from "vitest";
import type { Message } from "@/bindings";
import { activeThread, messageText, segments, thinkingOf } from "./thread";

function msg(id: string, parentId: string | null, text = id): Message {
  return {
    id,
    conversationId: "c",
    parentId,
    role: "user",
    content: [{ type: "text", text }],
    model: null,
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

  it("splits paragraphs and fenced code", () => {
    expect(segments("Intro\n\n```json\n{ }\n```\nAfter")).toEqual([
      { kind: "p", text: "Intro" },
      { kind: "code", lang: "json", code: "{ }" },
      { kind: "p", text: "After" },
    ]);
  });
});
