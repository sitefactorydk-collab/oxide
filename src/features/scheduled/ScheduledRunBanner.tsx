import { CalendarClock } from "lucide-react";
import { Link } from "react-router";
import { useScheduledTask, useTaskRunForChat } from "./queries";
import { formatWhen } from "./schedule";

/** Above a chat a scheduled task wrote: which task, when, and a link to its history. */
export function ScheduledRunBanner({ conversationId }: { conversationId: string }) {
  const { data: run } = useTaskRunForChat(conversationId);
  const { task } = useScheduledTask(run?.taskId);
  if (!run) return null;
  return (
    <p className="flex items-center gap-2 rounded-xl border border-line bg-sunken px-3.5 py-2 text-sm text-fg-muted">
      <CalendarClock className="size-4 shrink-0" />
      <span className="min-w-0 flex-1 truncate">
        Scheduled run of {task ? `“${task.name}”` : "a task"},{" "}
        {formatWhen(run.scheduledFor ?? run.startedAt)}
      </span>
      {task && (
        <Link to={`/scheduled/${task.id}`} className="shrink-0 text-accent hover:underline">
          View task
        </Link>
      )}
    </p>
  );
}
