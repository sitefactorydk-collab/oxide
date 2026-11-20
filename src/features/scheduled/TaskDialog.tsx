import { FolderPlus, X } from "lucide-react";
import { Dialog } from "radix-ui";
import { useMemo, useState, type ReactNode } from "react";
import type { ChatTools, ScheduledTask, TaskInput } from "@/bindings";
import { IconButton } from "@/components/ui/IconButton";
import { isUsable, modelNameFromId } from "@/features/models/models";
import { useModels, useNewChatModel } from "@/features/models/queries";
import { useProjects } from "@/features/projects/queries";
import { useSettings } from "@/features/settings/queries";
import { pickFolder } from "@/features/workspace/queries";
import { errorMessage } from "@/lib/api";
import { useDebounced } from "@/lib/useDebounced";
import { useCreateTask, useSchedulePreview, useUpdateTask } from "./queries";
import {
  WEEKDAY_NAMES,
  defaultScheduleForm,
  formatWhen,
  fromSchedule,
  toSchedule,
  type Frequency,
  type ScheduleForm,
} from "./schedule";

/** Built-in tool switches a task can turn off, as in the composer's "+" menu. */
const TOOL_SWITCHES: Array<{
  label: string;
  builtins: string[];
  setting?: "browser" | "chrome" | "sandbox" | "computer";
}> = [
  { label: "Web search and fetch", builtins: ["web_search", "web_fetch"] },
  { label: "Files in the shared folders", builtins: ["files"] },
  { label: "Artifacts", builtins: ["artifact"] },
  { label: "Browser", builtins: ["browser"], setting: "browser" },
  { label: "Chrome", builtins: ["chrome"], setting: "chrome" },
  { label: "Sandbox", builtins: ["sandbox"], setting: "sandbox" },
  {
    label: "Use the computer (always waits for your approval)",
    builtins: ["computer"],
    setting: "computer",
  },
  { label: "Save skills (always waits for your approval)", builtins: ["save_skill"] },
];

const FREQUENCIES: Array<{ value: Frequency; label: string }> = [
  { value: "once", label: "Once" },
  { value: "hourly", label: "Every hour" },
  { value: "daily", label: "Every day" },
  { value: "weekdays", label: "Weekdays (Mon–Fri)" },
  { value: "weekly", label: "Every week" },
  { value: "monthly", label: "Every month" },
  { value: "custom", label: "Custom (cron)" },
];

const field =
  "w-full rounded-lg border border-line bg-raised px-3 text-md text-fg outline-none transition-colors focus:border-accent";

/** Create a task (no `task`) or edit one. */
export function TaskDialog({
  open,
  onOpenChange,
  task,
  onSaved,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  task?: ScheduledTask;
  onSaved?: (task: ScheduledTask) => void;
}) {
  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-40 bg-overlay" />
        <Dialog.Content
          aria-describedby={undefined}
          className="ox-pop fixed left-1/2 top-1/2 z-50 flex max-h-[calc(100vh-48px)] w-[min(640px,calc(100vw-48px))] -translate-x-1/2 -translate-y-1/2 flex-col rounded-2xl border border-line bg-app shadow-lg outline-none"
        >
          <div className="flex items-center justify-between px-6 pb-3 pt-6">
            <Dialog.Title className="font-serif text-xl font-normal text-fg">
              {task ? "Edit task" : "New scheduled task"}
            </Dialog.Title>
            <Dialog.Close asChild>
              <IconButton tooltip={false} size="sm" label="Close">
                <X className="size-4" />
              </IconButton>
            </Dialog.Close>
          </div>
          {open && (
            <TaskForm
              task={task}
              onDone={(saved) => {
                onOpenChange(false);
                if (saved) onSaved?.(saved);
              }}
            />
          )}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

function TaskForm({
  task,
  onDone,
}: {
  task?: ScheduledTask;
  onDone: (saved?: ScheduledTask) => void;
}) {
  const create = useCreateTask();
  const update = useUpdateTask();
  const save = task ? update : create;
  const [defaultModel] = useNewChatModel();
  const { data: lists } = useModels();
  const { data: projects } = useProjects();
  const { data: settings } = useSettings();

  const [name, setName] = useState(task?.name ?? "");
  const [prompt, setPrompt] = useState(task?.prompt ?? "");
  const [sched, setSched] = useState<ScheduleForm>(() =>
    task ? fromSchedule(task.schedule) : defaultScheduleForm(),
  );
  const [model, setModel] = useState(task?.model ?? defaultModel);
  const [projectId, setProjectId] = useState<string | null>(task?.projectId ?? null);
  const [folders, setFolders] = useState<string[]>(task?.folders ?? []);
  const [tools, setTools] = useState<ChatTools>(
    task?.tools ?? { disabledServers: [], disabledTools: [], disabledBuiltins: ["save_skill"] },
  );
  const [thinking, setThinking] = useState(task?.thinking ?? false);
  const [approval, setApproval] = useState(task?.approval ?? "ask");
  const [missed, setMissed] = useState(task?.missed ?? "runOnce");
  const [folderError, setFolderError] = useState<string | null>(null);

  const schedule = useMemo(() => toSchedule(sched), [sched]);
  const debounced = useDebounced(schedule, 250);
  const preview = useSchedulePreview(debounced);
  const set = (patch: Partial<ScheduleForm>) => setSched((s) => ({ ...s, ...patch }));

  const usable = lists
    .map((l) => ({ ...l, models: l.models.filter(isUsable) }))
    .filter((l) => l.models.length > 0);
  const listed = usable.some((l) => l.models.some((m) => m.id === model));

  const builtinOff = (b: string) => tools.disabledBuiltins.includes(b);
  const setBuiltins = (names: string[], on: boolean) =>
    setTools((t) => ({
      ...t,
      disabledBuiltins: on
        ? t.disabledBuiltins.filter((b) => !names.includes(b))
        : [...new Set([...t.disabledBuiltins, ...names])],
    }));

  const submit = () => {
    const input: TaskInput = {
      name,
      prompt,
      schedule,
      model,
      projectId,
      folders,
      tools,
      thinking,
      approval,
      missed,
      enabled: task?.enabled ?? true,
    };
    if (task && !task.enabled && schedule.kind === "once") input.enabled = true;
    const onSuccess = (saved: ScheduledTask) => onDone(saved);
    if (task) update.mutate({ id: task.id, input }, { onSuccess });
    else create.mutate(input, { onSuccess });
  };

  return (
    <form
      onSubmit={(e) => {
        e.preventDefault();
        submit();
      }}
      className="flex min-h-0 flex-1 flex-col"
    >
      <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto px-6 pb-4">
        <Field label="Name">
          <input
            autoFocus
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="e.g. Morning news brief"
            className={`${field} h-10`}
          />
        </Field>
        <Field label="What should it do?">
          <textarea
            value={prompt}
            onChange={(e) => setPrompt(e.target.value)}
            rows={5}
            placeholder="Each run starts a new chat with this message."
            className={`${field} resize-y py-2`}
          />
        </Field>

        <fieldset className="flex flex-col gap-2">
          <legend className="mb-1.5 text-sm font-medium text-fg-muted">When</legend>
          <div className="flex flex-wrap items-center gap-2">
            <select
              aria-label="How often"
              value={sched.frequency}
              onChange={(e) => set({ frequency: e.target.value as Frequency })}
              className={`${field} h-10 w-auto`}
            >
              {FREQUENCIES.map((f) => (
                <option key={f.value} value={f.value}>
                  {f.label}
                </option>
              ))}
            </select>
            {sched.frequency === "once" && (
              <input
                type="date"
                aria-label="Date"
                value={sched.date}
                onChange={(e) => set({ date: e.target.value })}
                className={`${field} h-10 w-auto`}
              />
            )}
            {sched.frequency === "weekly" && (
              <select
                aria-label="Day of the week"
                value={sched.weekday}
                onChange={(e) => set({ weekday: Number(e.target.value) })}
                className={`${field} h-10 w-auto`}
              >
                {WEEKDAY_NAMES.map((d, i) => (
                  <option key={d} value={i}>
                    {d}
                  </option>
                ))}
              </select>
            )}
            {sched.frequency === "monthly" && (
              <label className="flex items-center gap-2 text-base text-fg-muted">
                on day
                <input
                  type="number"
                  aria-label="Day of the month"
                  min={1}
                  max={31}
                  value={sched.dayOfMonth}
                  onChange={(e) => set({ dayOfMonth: Number(e.target.value) || 1 })}
                  className={`${field} h-10 w-20`}
                />
              </label>
            )}
            {sched.frequency === "hourly" && (
              <label className="flex items-center gap-2 text-base text-fg-muted">
                at minute
                <input
                  type="number"
                  aria-label="Minute past the hour"
                  min={0}
                  max={59}
                  value={sched.minute}
                  onChange={(e) => set({ minute: Number(e.target.value) || 0 })}
                  className={`${field} h-10 w-20`}
                />
              </label>
            )}
            {sched.frequency !== "hourly" && sched.frequency !== "custom" && (
              <label className="flex items-center gap-2 text-base text-fg-muted">
                at
                <input
                  type="time"
                  aria-label="Time"
                  value={sched.time}
                  onChange={(e) => set({ time: e.target.value })}
                  className={`${field} h-10 w-auto`}
                />
              </label>
            )}
            {sched.frequency === "custom" && (
              <input
                aria-label="Cron expression"
                value={sched.cron}
                onChange={(e) => set({ cron: e.target.value })}
                placeholder="minute hour day month weekday, e.g. 0 9 * * 1-5"
                className={`${field} h-10 flex-1 font-mono text-sm`}
              />
            )}
          </div>
          <p
            aria-live="polite"
            className={preview.error ? "text-sm text-danger" : "text-sm text-fg-subtle"}
          >
            {preview.error
              ? errorMessage(preview.error)
              : preview.data && preview.data.length > 0
                ? `Next: ${preview.data.map((t) => formatWhen(t)).join(" · ")}`
                : preview.data
                  ? "That time has passed."
                  : " "}
          </p>
        </fieldset>

        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
          <Field label="Model">
            <select
              value={model}
              onChange={(e) => setModel(e.target.value)}
              className={`${field} h-10`}
            >
              {!listed && <option value={model}>{modelNameFromId(model) || "Pick a model"}</option>}
              {usable.map((l) => (
                <optgroup key={l.providerId} label={l.providerName}>
                  {l.models.map((m) => (
                    <option key={m.id} value={m.id}>
                      {m.label}
                    </option>
                  ))}
                </optgroup>
              ))}
            </select>
          </Field>
          <Field label="Project">
            <select
              value={projectId ?? ""}
              onChange={(e) => setProjectId(e.target.value || null)}
              className={`${field} h-10`}
            >
              <option value="">No project</option>
              {projects
                .filter((p) => !p.archived || p.id === projectId)
                .map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name}
                  </option>
                ))}
            </select>
          </Field>
        </div>

        <div className="flex flex-col gap-1.5">
          <span className="text-sm font-medium text-fg-muted">Folders</span>
          {folders.length > 0 && (
            <ul className="flex flex-wrap gap-1.5">
              {folders.map((f) => (
                <li
                  key={f}
                  className="inline-flex h-7 items-center gap-1 rounded-md border border-line bg-raised pl-2.5 text-sm text-fg"
                >
                  <span className="max-w-72 truncate" title={f}>
                    {f}
                  </span>
                  <button
                    type="button"
                    aria-label={`Remove ${f}`}
                    onClick={() => setFolders((all) => all.filter((x) => x !== f))}
                    className="inline-flex size-6 items-center justify-center rounded text-fg-muted hover:text-fg"
                  >
                    <X className="size-3.5" />
                  </button>
                </li>
              ))}
            </ul>
          )}
          <div>
            <button
              type="button"
              onClick={async () => {
                setFolderError(null);
                try {
                  const p = await pickFolder();
                  if (p) setFolders((all) => (all.includes(p) ? all : [...all, p]));
                } catch (e) {
                  setFolderError(errorMessage(e));
                }
              }}
              className="inline-flex h-8 items-center gap-2 rounded-lg border border-line px-3 text-sm text-fg-muted hover:bg-hover hover:text-fg"
            >
              <FolderPlus className="size-4" />
              Add folder
            </button>
          </div>
          {folderError && <p className="text-sm text-danger">{folderError}</p>}
          <p className="text-sm text-fg-subtle">
            Each run's chat gets these folders, as if you had shared them.
          </p>
        </div>

        <fieldset className="flex flex-col gap-1.5">
          <legend className="mb-1.5 text-sm font-medium text-fg-muted">Tools</legend>
          {TOOL_SWITCHES.filter((t) => !t.setting || settings?.[t.setting].enabled).map((t) => (
            <label key={t.label} className="flex items-center gap-2 text-base text-fg">
              <input
                type="checkbox"
                checked={!t.builtins.some(builtinOff)}
                onChange={(e) => setBuiltins(t.builtins, e.target.checked)}
              />
              {t.label}
            </label>
          ))}
          <label className="flex items-center gap-2 text-base text-fg">
            <input
              type="checkbox"
              checked={thinking}
              onChange={(e) => setThinking(e.target.checked)}
            />
            Extended thinking (models that support it)
          </label>
        </fieldset>

        <fieldset className="flex flex-col gap-1.5">
          <legend className="mb-1.5 text-sm font-medium text-fg-muted">
            Tool calls that ask for approval
          </legend>
          <Radio
            name="approval"
            checked={approval === "ask"}
            onChange={() => setApproval("ask")}
            label="Ask me"
            hint="The run waits on its approval card and Oxide notifies you."
          />
          <Radio
            name="approval"
            checked={approval === "auto"}
            onChange={() => setApproval("auto")}
            label="Run them without asking"
            hint="For tasks you trust: nobody checks what the model does. Saving a skill still asks."
          />
        </fieldset>

        <fieldset className="flex flex-col gap-1.5">
          <legend className="mb-1.5 text-sm font-medium text-fg-muted">
            If Oxide wasn't running at the time
          </legend>
          <Radio
            name="missed"
            checked={missed === "runOnce"}
            onChange={() => setMissed("runOnce")}
            label="Run once when Oxide is back"
          />
          <Radio
            name="missed"
            checked={missed === "skip"}
            onChange={() => setMissed("skip")}
            label="Skip it and wait for the next time"
          />
        </fieldset>

        {save.error && (
          <p role="alert" className="text-base text-danger">
            {errorMessage(save.error)}
          </p>
        )}
      </div>
      <div className="flex justify-end gap-2 border-t border-line px-6 py-4">
        <button
          type="button"
          onClick={() => onDone()}
          className="h-9 rounded-lg px-3.5 text-base text-fg-muted hover:bg-hover"
        >
          Cancel
        </button>
        <button
          type="submit"
          disabled={!name.trim() || !prompt.trim() || !model || save.isPending}
          className="h-9 rounded-lg bg-fg px-3.5 text-base font-medium text-app transition-opacity hover:opacity-90 disabled:opacity-40"
        >
          {task ? "Save" : "Create task"}
        </button>
      </div>
    </form>
  );
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="flex flex-col gap-1.5 text-sm font-medium text-fg-muted">
      {label}
      {children}
    </label>
  );
}

function Radio({
  name,
  checked,
  onChange,
  label,
  hint,
}: {
  name: string;
  checked: boolean;
  onChange: () => void;
  label: string;
  hint?: string;
}) {
  return (
    <label className="flex items-start gap-2 text-base text-fg">
      <input type="radio" name={name} checked={checked} onChange={onChange} className="mt-1" />
      <span>
        {label}
        {hint && <span className="block text-sm text-fg-subtle">{hint}</span>}
      </span>
    </label>
  );
}
