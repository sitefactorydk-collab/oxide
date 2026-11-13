import { ArrowLeft, Ellipsis, MessageSquare, Pause, Pencil, Play, Trash } from "lucide-react";
import { useState } from "react";
import { Link, useNavigate, useParams } from "react-router";
import type { ScheduledTask, TaskRun } from "@/bindings";
import { IconButton } from "@/components/ui/IconButton";
import { MenuContent, MenuItem, MenuRoot, MenuTrigger } from "@/components/ui/Menu";
import { useModelLabel } from "@/features/models/queries";
import { ConfirmDialog } from "@/features/projects/ProjectDialogs";
import { useProjects } from "@/features/projects/queries";
import { formatRelative } from "@/lib/time";
import {
  useDeleteTask,
  useRunTask,
  useScheduledTask,
  useSetTaskEnabled,
  useTaskRuns,
} from "./queries";
import { RunStatusBadge } from "./RunStatusBadge";
import { TRIGGER_LABEL, describeSchedule, formatWhen } from "./schedule";
import { TaskDialog } from "./TaskDialog";

/** One task: what it does, when it runs next, and its history (each run's output). */
export function TaskPage() {
  const { id = "" } = useParams();
  const { task, isPending } = useScheduledTask(id);
  if (!task) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center gap-3">
        {!isPending && (
          <>
            <p className="font-serif text-2xl text-fg">This task doesn't exist any more</p>
            <Link to="/scheduled" className="text-accent hover:underline">
              Back to Scheduled
            </Link>
          </>
        )}
      </div>
    );
  }
  return <TaskView task={task} />;
}

function TaskView({ task }: { task: ScheduledTask }) {
  const navigate = useNavigate();
  const [editing, setEditing] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const run = useRunTask();
  const setEnabled = useSetTaskEnabled();
  const del = useDeleteTask();
  const modelLabel = useModelLabel(task.model);
  const { data: projects } = useProjects();
  const project = projects.find((p) => p.id === task.projectId);
  const { data: runs, isPending } = useTaskRuns(task.id);
  const canResume = !(task.schedule.kind === "once" && task.lastRun);

  return (
    <div className="min-h-0 flex-1 overflow-y-auto">
      <div className="mx-auto w-full max-w-page px-6 py-10">
        <Link
          to="/scheduled"
          className="mb-4 inline-flex items-center gap-1.5 text-sm text-fg-muted hover:text-fg"
        >
          <ArrowLeft className="size-4" />
          Scheduled
        </Link>
        <div className="mb-2 flex items-start justify-between gap-4">
          <h1 className="font-serif text-2xl font-normal tracking-tight text-fg">{task.name}</h1>
          <div className="flex shrink-0 items-center gap-1">
            <button
              type="button"
              onClick={() => run.mutate(task.id)}
              disabled={run.isPending}
              className="inline-flex h-9 items-center gap-2 rounded-lg bg-fg px-3.5 text-base font-medium text-app transition-opacity hover:opacity-90 disabled:opacity-40 [&_svg]:size-4"
            >
              <Play />
              Run now
            </button>
            <IconButton label="Edit task" onClick={() => setEditing(true)}>
              <Pencil className="size-4" />
            </IconButton>
            <MenuRoot>
              <MenuTrigger asChild>
                <IconButton label="More" tooltip={false}>
                  <Ellipsis className="size-4" />
                </IconButton>
              </MenuTrigger>
              <MenuContent align="end">
                {task.enabled ? (
                  <MenuItem onSelect={() => setEnabled.mutate({ id: task.id, enabled: false })}>
                    <Pause />
                    Pause
                  </MenuItem>
                ) : (
                  canResume && (
                    <MenuItem onSelect={() => setEnabled.mutate({ id: task.id, enabled: true })}>
                      <Play />
                      Resume
                    </MenuItem>
                  )
                )}
                <MenuItem danger onSelect={() => setConfirming(true)}>
                  <Trash />
                  Delete
                </MenuItem>
              </MenuContent>
            </MenuRoot>
          </div>
        </div>
        <p className="mb-6 text-base text-fg-muted">
          {describeSchedule(task.schedule)} ·{" "}
          {task.enabled
            ? task.nextRunAt
              ? `next ${formatWhen(task.nextRunAt)}`
              : "nothing left to run"
            : canResume
              ? "paused"
              : "ran once"}
        </p>

        <dl className="mb-8 grid grid-cols-[max-content_1fr] gap-x-6 gap-y-2 rounded-xl border border-line bg-raised p-4 text-base">
          <dt className="text-fg-muted">Prompt</dt>
          <dd className="whitespace-pre-wrap text-fg">{task.prompt}</dd>
          <dt className="text-fg-muted">Model</dt>
          <dd className="text-fg">
            {modelLabel}
            {task.thinking && " · extended thinking"}
          </dd>
          {project && (
            <>
              <dt className="text-fg-muted">Project</dt>
              <dd className="text-fg">{project.name}</dd>
            </>
          )}
          {task.folders.length > 0 && (
            <>
              <dt className="text-fg-muted">Folders</dt>
              <dd className="flex flex-col text-fg">
                {task.folders.map((f) => (
                  <span key={f} className="truncate" title={f}>
                    {f}
                  </span>
                ))}
              </dd>
            </>
          )}
          <dt className="text-fg-muted">Approvals</dt>
          <dd className="text-fg">
            {task.approval === "auto"
              ? "Tools run without asking"
              : "Asks me (the run waits and Oxide notifies me)"}
          </dd>
          <dt className="text-fg-muted">Missed runs</dt>
          <dd className="text-fg">
            {task.missed === "runOnce" ? "Run once when Oxide is back" : "Skipped"}
          </dd>
        </dl>

        <h2 className="mb-3 text-md font-medium text-fg">History</h2>
        {!isPending && (runs ?? []).length === 0 && (
          <p className="text-base text-fg-muted">No runs yet.</p>
        )}
        <ol className="flex flex-col gap-3" aria-label="Runs">
          {(runs ?? []).map((r) => (
            <RunItem key={r.id} run={r} />
          ))}
        </ol>
      </div>
      <TaskDialog open={editing} onOpenChange={setEditing} task={task} />
      <ConfirmDialog
        open={confirming}
        onOpenChange={setConfirming}
        title={`Delete “${task.name}”?`}
        body="The task and its history are deleted. The chats its runs wrote stay."
        confirmLabel="Delete task"
        onConfirm={() => del.mutate(task.id, { onSuccess: () => navigate("/scheduled") })}
      />
    </div>
  );
}

function RunItem({ run }: { run: TaskRun }) {
  const [open, setOpen] = useState(false);
  const long = (run.output?.length ?? 0) > 600;
  const at = run.scheduledFor ?? run.startedAt;
  return (
    <li className="rounded-xl border border-line bg-raised p-4">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
        <span className="text-base font-medium text-fg" title={new Date(at).toLocaleString()}>
          {formatWhen(at)}
        </span>
        <RunStatusBadge status={run.status} />
        <span className="text-sm text-fg-subtle">
          {TRIGGER_LABEL[run.trigger]}
          {run.finishedAt && ` · ${formatRelative(run.finishedAt)}`}
        </span>
        {run.conversationId && (
          <Link
            to={`/chat/${run.conversationId}`}
            className="ml-auto inline-flex items-center gap-1.5 text-sm text-accent hover:underline"
          >
            <MessageSquare className="size-4" />
            {run.status === "waitingApproval" ? "Open the chat to approve" : "Open chat"}
          </Link>
        )}
      </div>
      {run.error && <p className="mt-2 text-base text-danger">{run.error}</p>}
      {run.status === "missed" && (
        <p className="mt-2 text-base text-fg-muted">
          Oxide wasn't running at this time, and this task skips missed runs.
        </p>
      )}
      {run.output && (
        <div className="mt-2">
          <p
            className={
              open || !long
                ? "whitespace-pre-wrap font-serif text-md text-fg"
                : "line-clamp-6 whitespace-pre-wrap font-serif text-md text-fg"
            }
          >
            {run.output}
          </p>
          {long && (
            <button
              type="button"
              onClick={() => setOpen((v) => !v)}
              className="mt-1 text-sm text-accent hover:underline"
            >
              {open ? "Show less" : "Show all"}
            </button>
          )}
        </div>
      )}
    </li>
  );
}
