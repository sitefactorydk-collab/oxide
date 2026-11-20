import { FileCode2, FileText, ImageOff, X } from "lucide-react";
import { Dialog } from "radix-ui";
import { useState } from "react";
import { IconButton } from "@/components/ui/IconButton";
import { attachmentPath, type AttachedBlock } from "@/features/chat/thread";
import { useAttachmentPreview } from "./queries";

/** Preview size for images in the thread (fits 2× DPI at the shown height). */
const THREAD_PREVIEW_SIDE = 480;

export function ImageThumb({ path, index }: { path: string; index: number }) {
  const [open, setOpen] = useState(false);
  const preview = useAttachmentPreview(path, THREAD_PREVIEW_SIDE);
  const full = useAttachmentPreview(path, null, open);
  const label = `Attached image ${index + 1}`;
  if (preview.isError) {
    return (
      <span
        role="img"
        aria-label={`${label} (missing)`}
        className="inline-flex h-24 w-32 items-center justify-center rounded-xl border border-line bg-sunken text-fg-subtle"
      >
        <ImageOff className="size-5" />
      </span>
    );
  }
  return (
    <Dialog.Root open={open} onOpenChange={setOpen}>
      <Dialog.Trigger asChild>
        <button
          type="button"
          aria-label={`Open ${label.toLowerCase()}`}
          className="overflow-hidden rounded-xl border border-line bg-sunken"
        >
          {preview.data ? (
            <img
              src={preview.data}
              alt={label}
              className="block max-h-40 max-w-60 object-contain"
            />
          ) : (
            <span className="block h-24 w-32 animate-pulse" />
          )}
        </button>
      </Dialog.Trigger>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-40 bg-overlay" />
        <Dialog.Content
          aria-describedby={undefined}
          className="ox-pop fixed inset-8 z-50 flex items-center justify-center outline-none"
          onClick={() => setOpen(false)}
        >
          <Dialog.Title className="sr-only">{label}</Dialog.Title>
          {(full.data ?? preview.data) && (
            <img
              src={full.data ?? preview.data}
              alt={label}
              className="max-h-full max-w-full rounded-lg shadow-lg"
              onClick={(e) => e.stopPropagation()}
            />
          )}
          <Dialog.Close asChild>
            <IconButton tooltip={false} label="Close" className="absolute right-0 top-0 bg-raised">
              <X className="size-4" />
            </IconButton>
          </Dialog.Close>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

/** Images and files sent with a user message, shown above its bubble. */
export function MessageAttachments({ blocks }: { blocks: AttachedBlock[] }) {
  if (blocks.length === 0) return null;
  let imageIndex = 0;
  return (
    <ul aria-label="Attached files" className="flex max-w-[85%] flex-wrap justify-end gap-2">
      {blocks.map((b, i) => {
        const path = attachmentPath(b);
        if (b.type === "image") {
          const n = imageIndex++;
          return (
            <li key={`${path}-${i}`}>
              <ImageThumb path={path} index={n} />
            </li>
          );
        }
        const isPdf = b.mediaType === "application/pdf";
        const Icon = isPdf ? FileText : FileCode2;
        return (
          <li
            key={`${path}-${i}`}
            className="flex h-12 max-w-64 items-center gap-2.5 rounded-lg border border-line bg-raised pl-1.5 pr-3 text-sm"
          >
            <span className="inline-flex size-9 shrink-0 items-center justify-center rounded-md bg-sunken text-fg-muted">
              <Icon aria-hidden className="size-4" />
            </span>
            <span className="flex min-w-0 flex-col leading-4">
              <span className="truncate text-fg">{b.name}</span>
              <span className="text-xs text-fg-subtle">{isPdf ? "PDF" : "Text"}</span>
            </span>
          </li>
        );
      })}
    </ul>
  );
}
