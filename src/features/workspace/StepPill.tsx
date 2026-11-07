import { CheckCircle2, ListChecks } from "lucide-react";
import { Tooltip } from "@/components/ui/Tooltip";
import { cn } from "@/lib/cn";
import { useTaskList } from "./queries";
import { useSessionPanel } from "./store";
import { stepLabel, taskProgress } from "./tasks";

/** "Step n of m" in the title bar while a chat has a task list; opens the session panel. */
export function StepPill({ conversationId }: { conversationId: string }) {
  const { data } = useTaskList(conversationId);
  const panel = useSessionPanel(conversationId);
  const items = data?.items ?? [];
  const label = stepLabel(items);
  if (!label) return null;
  const p = taskProgress(items);
  const finished = p.current === null;
  const current = p.current === null ? null : items[p.current];
  return (
    <Tooltip label={current ? current.title : "Every step is done"}>
      <button
        type="button"
        aria-label={`${label}. Show progress`}
        onClick={() => panel.setOpen(true)}
        className={cn(
          "inline-flex h-7 shrink-0 items-center gap-1.5 rounded-full border px-2.5 text-sm",
          finished
            ? "border-line text-fg-muted hover:bg-hover"
            : "border-accent bg-accent-soft text-fg hover:bg-hover",
        )}
      >
        {finished ? (
          <CheckCircle2 aria-hidden className="size-3.5 text-accent" />
        ) : (
          <ListChecks aria-hidden className="size-3.5 text-accent" />
        )}
        <span className="tabular-nums">{label}</span>
      </button>
    </Tooltip>
  );
}
