import type { AttachmentLimits } from "@/bindings";

/** Used until the backend's limits have loaded (same values as `ATTACHMENT_LIMITS`). */
export const DEFAULT_LIMITS: AttachmentLimits = {
  maxFiles: 20,
  maxImageBytes: 30 * 1024 * 1024,
  maxImageSide: 1568,
  maxPdfBytes: 32 * 1024 * 1024,
  maxPdfPages: 100,
  maxTextBytes: 1024 * 1024,
};

/** "512 B", "820 KB", "4.2 MB". */
export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  const mb = bytes / 1024 / 1024;
  return `${Number.isInteger(mb) ? mb : mb.toFixed(1)} MB`;
}

/** One line for the UI. */
export function limitsSummary(l: AttachmentLimits): string {
  return (
    `Up to ${l.maxFiles} files. Images up to ${formatBytes(l.maxImageBytes)}, ` +
    `PDFs up to ${formatBytes(l.maxPdfBytes)} and ${l.maxPdfPages} pages, ` +
    `text and code up to ${formatBytes(l.maxTextBytes)}.`
  );
}

/** The largest file of any kind. */
export function maxAnyBytes(l: AttachmentLimits): number {
  return Math.max(l.maxImageBytes, l.maxPdfBytes, l.maxTextBytes);
}
