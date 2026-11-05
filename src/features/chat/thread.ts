import type { ContentBlock, Message } from "@/bindings";

/**
 * The visible thread: the path from the root to the newest message, or to `leafId`.
 *
 * Messages form a tree (edits and regenerated replies add siblings). The backend returns every
 * message in insertion order, so the last one is the newest leaf, and it is always on the
 * branch the user last worked on. While a reply is written, the thread ends at the message it
 * answers (`leafId`), so a reply being regenerated replaces the old one on screen. The
 * branch navigator (‹ 1/2 ›) passes the leaf of the branch the user picked.
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

/**
 * Versions of a message: messages under the same parent with the same role (edits of a user
 * message, or regenerated replies), oldest first.
 */
export function siblingsOf(messages: Message[], m: Message): Message[] {
  return messages.filter((x) => x.parentId === m.parentId && x.role === m.role);
}

/**
 * The newest message in the subtree under `rootId`. Messages are in insertion order and a
 * child always comes after its parent, so this is a leaf: where the user last left that
 * branch.
 */
export function newestLeafUnder(messages: Message[], rootId: string): string {
  const inTree = new Set([rootId]);
  let newest = rootId;
  for (const m of messages) {
    if (m.parentId !== null && inTree.has(m.parentId)) {
      inTree.add(m.id);
      newest = m.id;
    }
  }
  return newest;
}

/** Images and documents attached to a message. */
export type AttachedBlock = Extract<ContentBlock, { type: "image" } | { type: "document" }>;

export function attachmentsOf(m: Pick<Message, "content">): AttachedBlock[] {
  return m.content.filter((b): b is AttachedBlock => b.type === "image" || b.type === "document");
}

/** The stored path of an attachment block (empty for inline data). */
export function attachmentPath(b: AttachedBlock): string {
  return b.source.kind === "path" ? b.source.path : "";
}
