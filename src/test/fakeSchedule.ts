import type {
  CommandError,
  Result,
  ScheduleChange,
  ScheduledTask,
  TaskInput,
  TaskRun,
  TaskSchedule,
  commands as RealCommands,
} from "@/bindings";

/**
 * Scheduled tasks in the fake backend. Runs don't start chats here: a test makes a run happen
 * with `fakeRun()` (which also sends the `scheduleChanged` event the real scheduler sends).
 */
interface FakeSchedule {
  tasks: ScheduledTask[];
  runs: TaskRun[];
  /** Inputs saved, in order (create and update). */
  saved: TaskInput[];
  /** Tasks "Run now" was pressed for. */
  runNow: string[];
}

let sc: FakeSchedule;
let seq = 0;
let emit: (c: ScheduleChange) => void = () => {};

export function setScheduleEmitter(f: (c: ScheduleChange) => void): void {
  emit = f;
}

export function resetFakeSchedule(): void {
  sc = { tasks: [], runs: [], saved: [], runNow: [] };
  seq = 0;
}

export function fakeScheduleState(): Readonly<FakeSchedule> {
  return sc;
}

const ok = <T>(data: T): Promise<Result<T, CommandError>> =>
  Promise.resolve({ status: "ok", data });
const fail = (kind: CommandError["kind"], message: string): Promise<Result<never, CommandError>> =>
  Promise.resolve({ status: "error", error: { kind, message } });

const HOUR = 3_600_000;

/** Three made-up upcoming times for a valid schedule (an error sentence for a bad one). */
function upcoming(s: TaskSchedule, now = Date.now()): number[] | string {
  if (s.kind === "once") {
    if (s.at <= 0) return "Pick a date and time.";
    return s.at > now ? [s.at] : [];
  }
  const expr = s.expr.trim();
  if (expr.startsWith("@")) return [now + HOUR, now + 2 * HOUR, now + 3 * HOUR];
  if (expr.split(/\s+/).length !== 5) {
    return "A cron schedule has five parts: minute, hour, day of the month, month and day of the week (for example “0 9 * * 1-5”).";
  }
  return [now + HOUR, now + 25 * HOUR, now + 49 * HOUR];
}

function check(input: TaskInput): string | null {
  if (!input.name.trim()) return "Give the task a name.";
  if (!input.prompt.trim()) return "Write what the task should do.";
  if (!input.model.trim()) return "Pick a model.";
  const next = upcoming(input.schedule);
  if (typeof next === "string") return next;
  if (input.schedule.kind === "once" && next.length === 0) {
    return "That time has passed. Pick a time in the future.";
  }
  return null;
}

function withLast(t: ScheduledTask): ScheduledTask {
  const last = sc.runs.filter((r) => r.taskId === t.id).at(-1) ?? null;
  return { ...t, lastRun: last };
}

function stored(id: string): ScheduledTask | undefined {
  return sc.tasks.find((t) => t.id === id);
}

function apply(t: ScheduledTask, input: TaskInput): ScheduledTask {
  const enabled = input.enabled ?? true;
  const next = upcoming(input.schedule);
  return {
    ...t,
    name: input.name.trim(),
    prompt: input.prompt.trim(),
    schedule: input.schedule,
    model: input.model,
    projectId: input.projectId ?? null,
    folders: input.folders ?? [],
    tools: input.tools ?? { disabledServers: [], disabledTools: [], disabledBuiltins: [] },
    thinking: input.thinking ?? false,
    approval: input.approval ?? "ask",
    missed: input.missed ?? "runOnce",
    enabled,
    nextRunAt: enabled && typeof next !== "string" ? (next[0] ?? null) : null,
    updatedAt: Date.now(),
  };
}

/** Adds a task directly (as if created earlier) and tells the app. */
export function seedTask(input: TaskInput): ScheduledTask {
  const t = addTask(input);
  emit({ taskId: t.id, run: null, deleted: false });
  return t;
}

function addTask(input: TaskInput): ScheduledTask {
  seq += 1;
  const now = Date.now();
  const blank: ScheduledTask = {
    id: `task${seq}`,
    name: "",
    prompt: "",
    schedule: input.schedule,
    model: input.model,
    projectId: null,
    folders: [],
    tools: { disabledServers: [], disabledTools: [], disabledBuiltins: [] },
    thinking: false,
    approval: "ask",
    missed: "runOnce",
    enabled: true,
    nextRunAt: null,
    lastRun: null,
    createdAt: now,
    updatedAt: now,
  };
  const t = apply(blank, input);
  sc.tasks.push(t);
  sc.tasks.sort((a, b) => a.name.localeCompare(b.name));
  return t;
}

/** A run of a task happened (or changed), as the scheduler would report it. */
export function fakeRun(taskId: string, patch: Partial<TaskRun> = {}): TaskRun {
  seq += 1;
  const existing = patch.id ? sc.runs.find((r) => r.id === patch.id) : undefined;
  const run: TaskRun = existing
    ? { ...existing, ...patch }
    : {
        id: `run${seq}`,
        taskId,
        conversationId: null,
        trigger: "schedule",
        scheduledFor: Date.now(),
        startedAt: Date.now(),
        finishedAt: null,
        status: "running",
        output: null,
        error: null,
        ...patch,
      };
  sc.runs = existing ? sc.runs.map((r) => (r.id === run.id ? run : r)) : [...sc.runs, run];
  emit({ taskId, run, deleted: false });
  return run;
}

export const fakeScheduleCommands: Pick<
  typeof RealCommands,
  | "listScheduledTasks"
  | "getScheduledTask"
  | "createScheduledTask"
  | "updateScheduledTask"
  | "setScheduledTaskEnabled"
  | "deleteScheduledTask"
  | "runScheduledTask"
  | "scheduledTaskRuns"
  | "previewSchedule"
  | "taskRunForChat"
> = {
  listScheduledTasks: () => ok(sc.tasks.map(withLast)),
  getScheduledTask: (id) => {
    const t = stored(id);
    return t ? ok(withLast(t)) : fail("notFound", "task not found");
  },
  createScheduledTask: (input) => {
    const problem = check(input);
    if (problem) return fail("invalid", problem);
    sc.saved.push(input);
    const t = addTask(input);
    emit({ taskId: t.id, run: null, deleted: false });
    return ok(withLast(t));
  },
  updateScheduledTask: (id, input) => {
    const t = stored(id);
    if (!t) return fail("notFound", "task not found");
    const problem = check(input);
    if (problem) return fail("invalid", problem);
    sc.saved.push(input);
    const u = apply(t, input);
    sc.tasks = sc.tasks.map((x) => (x.id === id ? u : x));
    emit({ taskId: id, run: null, deleted: false });
    return ok(withLast(u));
  },
  setScheduledTaskEnabled: (id, enabled) => {
    const t = stored(id);
    if (!t) return fail("notFound", "task not found");
    const next = upcoming(t.schedule);
    const u: ScheduledTask = {
      ...t,
      enabled,
      nextRunAt: enabled && typeof next !== "string" ? (next[0] ?? null) : null,
    };
    sc.tasks = sc.tasks.map((x) => (x.id === id ? u : x));
    emit({ taskId: id, run: null, deleted: false });
    return ok(withLast(u));
  },
  deleteScheduledTask: (id) => {
    if (!stored(id)) return fail("notFound", "task not found");
    sc.tasks = sc.tasks.filter((t) => t.id !== id);
    sc.runs = sc.runs.filter((r) => r.taskId !== id);
    emit({ taskId: id, run: null, deleted: true });
    return ok(null);
  },
  runScheduledTask: (id) => {
    if (!stored(id)) return fail("notFound", "task not found");
    sc.runNow.push(id);
    return ok(fakeRun(id, { trigger: "manual", scheduledFor: null }));
  },
  scheduledTaskRuns: (taskId) =>
    ok(
      sc.runs
        .filter((r) => r.taskId === taskId)
        .slice()
        .reverse(),
    ),
  previewSchedule: (schedule) => {
    const next = upcoming(schedule);
    return typeof next === "string" ? fail("invalid", next) : ok(next);
  },
  taskRunForChat: (conversationId) =>
    ok(sc.runs.filter((r) => r.conversationId === conversationId).at(-1) ?? null),
};
