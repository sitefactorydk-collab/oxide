import { CalendarClock, Info, Play, Plus } from "lucide-react";
import { useState } from "react";
import { Link, useNavigate } from "react-router";
import type { ScheduledTask } from "@/bindings";
import { PageHeader, PrimaryButton } from "@/components/PageHeader";
import { IconButton } from "@/components/ui/IconButton";
import { Switch } from "@/features/mcp/ui";
import { useSettings } from "@/features/settings/queries";
import { useOpenSettings } from "@/features/settings/useOpenSettings";
import { useRunTask, useScheduledTasks, useSetTaskEnabled } from "./queries";
import { RunStatusBadge } from "./RunStatusBadge";
import { describeSchedule, formatWhen } from "./schedule";
import { TaskDialog } from "./TaskDialog";

/** Scheduled tasks: saved prompts that run on their own, each run in a new chat. */
export function ScheduledPage() {
  const { data: tasks, isPending } = useScheduledTasks();
  const [creating, setCreating] = useState(false);
  const navigate = useNavigate();

  return (
    <div className="min-h-0 flex-1 overflow-y-auto">
      <div className="mx-auto w-full max-w-page px-6 py-10">
        <PageHeader
          title="Scheduled"
          action={
            <PrimaryButton onClick={() => setCreating(true)}>
              <Plus />
              New task
            </PrimaryButton>
          }
        />
        <RunsWhileOpenNote />
        {tasks.length > 0 && (
          <ul className="flex flex-col divide-y divide-line overflow-hidden rounded-xl border border-line bg-raised">
            {tasks.map((t) => (
              <TaskRow key={t.id} task={t} />
            ))}
          </ul>
        )}
        {!isPending && tasks.length === 0 && (
          <div className="rounded-xl border border-dashed border-line-strong p-8 text-center">
            <CalendarClock className="mx-auto mb-3 size-6 text-fg-subtle" />
            <p className="text-md text-fg">No scheduled tasks yet</p>
            <p className="mt-1 text-base text-fg-muted">
              A task runs a prompt on its own — every morning, on weekdays, once next Friday — each
              time in a new chat you can read later.
            </p>
          </div>
        )}
      </div>
      <TaskDialog
        open={creating}
        onOpenChange={setCreating}
        onSaved={(t) => navigate(`/scheduled/${t.id}`)}
      />
    </div>
  );
}

/** Tasks run only while Oxide does: say so when closing the window quits it. */
function RunsWhileOpenNote() {
  const { data: settings } = useSettings();
  const openSettings = useOpenSettings();
  if (!settings) return null;
  const { closeToTray, launchAtLogin } = settings.desktop;
  if (closeToTray && launchAtLogin) return null;
  return (
    <p className="mb-4 flex items-start gap-2 rounded-xl border border-line bg-sunken px-3.5 py-2.5 text-base text-fg-muted">
      <Info className="mt-0.5 size-4 shrink-0" />
      <span>
        Tasks run while Oxide is open or in the tray.{" "}
        {!closeToTray && "Closing the window quits Oxide, so tasks stop with it. "}
        {!launchAtLogin && "Oxide doesn't start when you sign in. "}
        <button
          type="button"
          onClick={() => openSettings("desktop")}
          className="text-accent hover:underline"
        >
          Settings → Desktop
        </button>
      </span>
    </p>
  );
}

function TaskRow({ task }: { task: ScheduledTask }) {
  const setEnabled = useSetTaskEnabled();
  const run = useRunTask();
  const when = task.enabled
    ? task.nextRunAt
      ? `Next: ${formatWhen(task.nextRunAt)}`
      : "Nothing left to run"
    : task.schedule.kind === "once" && task.lastRun
      ? "Ran once"
      : "Paused";
  return (
    <li className="flex items-center gap-3 px-4 py-3">
      <Link to={`/scheduled/${task.id}`} className="flex min-w-0 flex-1 flex-col gap-0.5">
        <span className="truncate text-md font-medium text-fg">{task.name}</span>
        <span className="truncate text-sm text-fg-muted">
          {describeSchedule(task.schedule)} · {when}
        </span>
      </Link>
      {task.lastRun && <RunStatusBadge status={task.lastRun.status} />}
      <IconButton
        label={`Run ${task.name} now`}
        onClick={() => run.mutate(task.id)}
        disabled={run.isPending}
      >
        <Play className="size-4" />
      </IconButton>
      <Switch
        checked={task.enabled}
        label={task.enabled ? `Pause ${task.name}` : `Resume ${task.name}`}
        onChange={(on) => setEnabled.mutate({ id: task.id, enabled: on })}
      />
    </li>
  );
}
