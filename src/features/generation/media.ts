import type { ContentBlock } from "@/bindings";

/** Whether a block is generated audio or video stored on disk. */
export function isPlayable(
  b: ContentBlock,
): b is Extract<ContentBlock, { type: "document" }> & { source: { kind: "path"; path: string } } {
  return (
    b.type === "document" &&
    b.source.kind === "path" &&
    (b.mediaType.startsWith("audio/") || b.mediaType.startsWith("video/"))
  );
}
