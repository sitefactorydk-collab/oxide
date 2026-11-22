import { Zap } from "lucide-react";
import type { AutoMark } from "@/bindings";
import { Tooltip } from "@/components/ui/Tooltip";
import { cn } from "@/lib/cn";
import { AUTO_MARK_HINTS, AUTO_MARK_LABELS } from "./labels";

/** The small **Auto** mark on a tool line run (or stopped) in Auto. */
export function AutoMarkBadge({ mark }: { mark: AutoMark }) {
  return (
    <Tooltip label={AUTO_MARK_HINTS[mark]}>
      <span
        aria-label={`Auto: ${AUTO_MARK_LABELS[mark]}`}
        className={cn(
          "inline-flex shrink-0 items-center gap-1 rounded px-1.5 py-0.5 text-[11px] leading-none",
          mark === "stopped" ? "bg-accent-soft text-accent" : "bg-sunken text-fg-subtle",
        )}
      >
        <Zap aria-hidden className="size-3" />
        Auto · {AUTO_MARK_LABELS[mark]}
      </span>
    </Tooltip>
  );
}

/** The mark in a compact tool row (Phase 24): a small ⚡ with the mark's text in a tooltip. */
export function AutoMarkIcon({ mark }: { mark: AutoMark }) {
  return (
    <Tooltip
      label={
        <span className="max-w-72 font-normal">
          <strong className="font-semibold">Auto · {AUTO_MARK_LABELS[mark]}</strong> —{" "}
          {AUTO_MARK_HINTS[mark]}
        </span>
      }
    >
      <span
        role="img"
        aria-label={`Auto: ${AUTO_MARK_LABELS[mark]}`}
        className={cn(
          "inline-flex shrink-0 items-center",
          mark === "stopped" ? "text-accent" : "text-fg-subtle",
        )}
      >
        <Zap aria-hidden className="size-3.5" />
      </span>
    </Tooltip>
  );
}
