import type { ContentBlock, Message } from "@/bindings";

/**
 * The visible thread: the path from the root to the newest message, or to `leafId`.
 *
 * Messages form a tree (edits and regenerated replies add siblings). The backend returns every
 * message in insertion order, so the last one is the newest leaf, and it is always on the
 * branch the user last worked on. While a reply is written, the thread ends at the message it
 * answers (`leafId`), so a reply being regenerated replaces the old one on screen. Branch
 * navigation (‹ 1/2 ›) arrives in Phase 4.
 */
export function activeThread(messages: Message[], leafId?: string): Message[] {
  const leaf =
    (leafId !== undefined ? messages.find((m) => m.id === leafId) : undefined) ??
    messages[messages.length - 1];
  if (!leaf) return [];
  const byId = new Map(messages.map((m) => [m.id, m]));
  const path: Message[] = [];
  const seen = new Set<string>();
  let cur: Message | undefined = leaf;
  while (cur && !seen.has(cur.id)) {
    seen.add(cur.id);
    path.push(cur);
    cur = cur.parentId ? byId.get(cur.parentId) : undefined;
  }
  return path.reverse();
}

/** Plain text of a message (text blocks only), for copy, edit and previews. */
export function messageText(m: Pick<Message, "content">): string {
  return blocksText(m.content);
}

export function blocksText(content: ContentBlock[]): string {
  return content
    .flatMap((b) => (b.type === "text" ? [b.text] : []))
    .filter((t) => t.trim() !== "")
    .join("\n\n");
}

export interface ThinkingInfo {
  seconds: number;
  text: string;
}

/** Combined extended-thinking output of a message, if any. */
export function thinkingOf(m: Pick<Message, "content">): ThinkingInfo | undefined {
  const blocks = m.content.filter(
    (b): b is Extract<ContentBlock, { type: "thinking" }> => b.type === "thinking",
  );
  if (blocks.length === 0) return undefined;
  const ms = blocks.reduce((sum, b) => sum + (b.durationMs ?? 0), 0);
  return {
    seconds: Math.max(1, Math.round(ms / 1000)),
    text: blocks.map((b) => b.text).join("\n\n"),
  };
}

/** A paragraph or a fenced code block. Phase 4 replaces this with full Markdown. */
export type Segment = { kind: "p"; text: string } | { kind: "code"; lang: string; code: string };

/** Splits text into paragraphs and ``` fenced code blocks. */
export function segments(text: string): Segment[] {
  const out: Segment[] = [];
  const lines = text.replace(/\r\n/g, "\n").split("\n");
  let para: string[] = [];
  const flush = () => {
    const t = para.join("\n").trim();
    if (t) out.push({ kind: "p", text: t });
    para = [];
  };
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i] ?? "";
    const fence = /^```\s*([\w+#.-]*)\s*$/.exec(line);
    if (fence) {
      flush();
      const code: string[] = [];
      i++;
      while (i < lines.length && !/^```\s*$/.test(lines[i] ?? "")) {
        code.push(lines[i] ?? "");
        i++;
      }
      out.push({ kind: "code", lang: fence[1] || "text", code: code.join("\n") });
    } else if (line.trim() === "") {
      flush();
    } else {
      para.push(line);
    }
  }
  flush();
  return out;
}
