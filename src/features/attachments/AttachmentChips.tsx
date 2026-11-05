import {
  AlertCircle,
  FileCode2,
  FileText,
  Image as ImageIcon,
  LoaderCircle,
  X,
} from "lucide-react";
import { Fragment } from "react";
import type { Attachment, ModelCapabilities, ProviderKind } from "@/bindings";
import { Tooltip } from "@/components/ui/Tooltip";
import { cn } from "@/lib/cn";
import { formatBytes } from "./limits";
import type { PendingAttachment } from "./useAttachments";
import { modelWarning } from "./warnings";

function detail(a: Attachment): string {
  switch (a.kind) {
    case "image":
      return a.width && a.height ? `${a.width}×${a.height}` : formatBytes(a.size);
    case "pdf":
      return a.pages !== null
        ? `PDF · ${a.pages} page${a.pages === 1 ? "" : "s"}`
        : `PDF · ${formatBytes(a.size)}`;
    case "text":
      return formatBytes(a.size);
  }
}

/** The composer's attachment chips. */
export function AttachmentChips({
  items,
  onRemove,
  caps,
  providerKind,
}: {
  items: PendingAttachment[];
  onRemove: (key: string) => void;
  caps?: ModelCapabilities;
  providerKind?: ProviderKind;
}) {
  return (
    <ul aria-label="Attachments" className="mb-2 flex flex-wrap gap-2">
      {items.map((item) => {
        const a = item.attachment;
        const warning = a ? modelWarning(a, caps, providerKind) : null;
        const tip = item.error ?? [a?.note, warning].filter(Boolean).join(" ");
        const Icon =
          item.status === "error"
            ? AlertCircle
            : a?.kind === "image"
              ? ImageIcon
              : a?.kind === "text"
                ? FileCode2
                : FileText;
        const chip = (
          <li
            aria-busy={item.status === "loading"}
            aria-invalid={item.status === "error"}
            className={cn(
              "group flex h-12 max-w-64 items-center gap-2.5 rounded-lg border bg-sunken pl-1.5 pr-1 text-sm",
              item.status === "error" ? "border-danger bg-danger-soft" : "border-line",
              warning && "border-line-strong",
            )}
          >
            {a?.thumbnail ? (
              <img
                src={a.thumbnail}
                alt=""
                className="size-9 shrink-0 rounded-md border border-line object-cover"
              />
            ) : (
              <span
                className={cn(
                  "inline-flex size-9 shrink-0 items-center justify-center rounded-md bg-raised",
                  item.status === "error" ? "text-danger" : "text-fg-muted",
                )}
              >
                {item.status === "loading" ? (
                  <LoaderCircle aria-hidden className="size-4 animate-spin" />
                ) : (
                  <Icon aria-hidden className="size-4" />
                )}
              </span>
            )}
            <span className="flex min-w-0 flex-col leading-4">
              <span className="truncate text-fg">{item.name}</span>
              <span
                className={cn(
                  "truncate text-xs",
                  item.status === "error" ? "text-danger" : "text-fg-subtle",
                )}
              >
                {item.status === "loading"
                  ? "Preparing…"
                  : item.status === "error"
                    ? "Can’t attach"
                    : a
                      ? `${detail(a)}${warning ? " · see note" : ""}`
                      : ""}
              </span>
            </span>
            <button
              type="button"
              aria-label={`Remove ${item.name}`}
              onClick={() => onRemove(item.key)}
              className="inline-flex size-6 shrink-0 items-center justify-center rounded-md text-fg-subtle hover:bg-hover hover:text-fg"
            >
              <X className="size-3.5" />
            </button>
          </li>
        );
        return tip ? (
          <Tooltip key={item.key} label={<span className="max-w-72 font-normal">{tip}</span>}>
            {chip}
          </Tooltip>
        ) : (
          <Fragment key={item.key}>{chip}</Fragment>
        );
      })}
    </ul>
  );
}
