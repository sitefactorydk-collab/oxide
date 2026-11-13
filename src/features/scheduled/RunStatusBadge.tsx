import {
  Ban,
  CircleCheck,
  CircleStop,
  CircleX,
  Hourglass,
  LoaderCircle,
  SkipForward,
} from "lucide-react";
import type { RunStatus } from "@/bindings";
import { cn } from "@/lib/cn";
import { STATUS_LABEL } from "./schedule";

const ICON = {
  running: LoaderCircle,
  waitingApproval: Hourglass,
  done: CircleCheck,
  failed: CircleX,
  stopped: CircleStop,
  missed: Ban,
  skipped: SkipForward,
} satisfies Record<RunStatus, unknown>;

/** A run's status as a small coloured label. */
export function RunStatusBadge({ status }: { status: RunStatus }) {
  const Icon = ICON[status];
  return (
    <span
      className={cn(
        "inline-flex h-6 shrink-0 items-center gap-1 rounded-md px-2 text-xs font-medium",
        status === "done" && "bg-sunken text-success",
        status === "failed" && "bg-sunken text-danger",
        status === "waitingApproval" && "bg-sunken text-accent",
        status === "running" && "bg-sunken text-fg",
        (status === "stopped" || status === "missed" || status === "skipped") &&
          "bg-sunken text-fg-subtle",
      )}
    >
      <Icon className={cn("size-3.5", status === "running" && "animate-spin")} />
      {STATUS_LABEL[status]}
    </span>
  );
}
