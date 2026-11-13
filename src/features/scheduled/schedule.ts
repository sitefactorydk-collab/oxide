import type { RunStatus, RunTrigger, TaskSchedule } from "@/bindings";

/** How often a task runs, as picked in the task dialog. "custom" is a cron expression. */
export type Frequency = "once" | "hourly" | "daily" | "weekdays" | "weekly" | "monthly" | "custom";

/** The schedule fields of the task dialog. Times are local ("HH:MM"). */
export interface ScheduleForm {
  frequency: Frequency;
  /** "HH:MM" (daily, weekdays, weekly, monthly, once). */
  time: string;
  /** Minute past the hour (hourly). */
  minute: number;
  /** 0 = Sunday … 6 = Saturday (weekly). */
  weekday: number;
  /** 1–31 (monthly; months without that day are skipped). */
  dayOfMonth: number;
  /** "YYYY-MM-DD" (once). */
  date: string;
  /** Five-field cron (custom). */
  cron: string;
}

export const WEEKDAY_NAMES = [
  "Sunday",
  "Monday",
  "Tuesday",
  "Wednesday",
  "Thursday",
  "Friday",
  "Saturday",
];

const pad = (n: number) => String(n).padStart(2, "0");

/** "YYYY-MM-DD" of a local date. */
export function localDate(d: Date): string {
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/** A new task's schedule: every day at 09:00 (once: tomorrow at 09:00). */
export function defaultScheduleForm(now: Date = new Date()): ScheduleForm {
  const tomorrow = new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1);
  return {
    frequency: "daily",
    time: "09:00",
    minute: 0,
    weekday: 1,
    dayOfMonth: 1,
    date: localDate(tomorrow),
    cron: "0 9 * * *",
  };
}

function hm(time: string): [number, number] {
  const m = /^(\d{1,2}):(\d{2})$/.exec(time.trim());
  if (!m) return [9, 0];
  return [Math.min(23, Number(m[1])), Math.min(59, Number(m[2]))];
}

/** The schedule the dialog's fields describe. */
export function toSchedule(f: ScheduleForm): TaskSchedule {
  const [h, m] = hm(f.time);
  switch (f.frequency) {
    case "once": {
      const at = new Date(`${f.date}T${pad(h)}:${pad(m)}:00`).getTime();
      return { kind: "once", at: Number.isNaN(at) ? 0 : at };
    }
    case "hourly":
      return { kind: "cron", expr: `${Math.min(59, Math.max(0, f.minute))} * * * *` };
    case "daily":
      return { kind: "cron", expr: `${m} ${h} * * *` };
    case "weekdays":
      return { kind: "cron", expr: `${m} ${h} * * 1-5` };
    case "weekly":
      return { kind: "cron", expr: `${m} ${h} * * ${f.weekday}` };
    case "monthly":
      return { kind: "cron", expr: `${m} ${h} ${Math.min(31, Math.max(1, f.dayOfMonth))} * *` };
    case "custom":
      return { kind: "cron", expr: f.cron.trim().split(/\s+/).join(" ") };
  }
}

const NUM = /^\d{1,2}$/;

/** The dialog's fields for a saved schedule; cron expressions it can't show become "custom". */
export function fromSchedule(s: TaskSchedule, now: Date = new Date()): ScheduleForm {
  const base = defaultScheduleForm(now);
  if (s.kind === "once") {
    const d = new Date(s.at);
    return {
      ...base,
      frequency: "once",
      date: localDate(d),
      time: `${pad(d.getHours())}:${pad(d.getMinutes())}`,
    };
  }
  const expr = s.expr.trim().split(/\s+/).join(" ");
  const custom = { ...base, frequency: "custom" as const, cron: expr };
  const parts = expr.split(" ");
  if (parts.length !== 5) return custom;
  const [mi = "", ho = "", dom = "", mon = "", dow = ""] = parts;
  if (!NUM.test(mi) || Number(mi) > 59 || mon !== "*") return custom;
  const minute = Number(mi);
  if (ho === "*" && dom === "*" && dow === "*") {
    return { ...base, frequency: "hourly", minute, cron: expr };
  }
  if (!NUM.test(ho) || Number(ho) > 23) return custom;
  const time = `${pad(Number(ho))}:${pad(minute)}`;
  if (dom === "*" && dow === "*") return { ...base, frequency: "daily", time, cron: expr };
  if (dom === "*" && dow === "1-5") return { ...base, frequency: "weekdays", time, cron: expr };
  if (dom === "*" && /^[0-7]$/.test(dow)) {
    return { ...base, frequency: "weekly", time, weekday: Number(dow) % 7, cron: expr };
  }
  if (NUM.test(dom) && Number(dom) >= 1 && Number(dom) <= 31 && dow === "*") {
    return { ...base, frequency: "monthly", time, dayOfMonth: Number(dom), cron: expr };
  }
  return custom;
}

function ordinal(n: number): string {
  const rem = n % 100;
  if (rem >= 11 && rem <= 13) return `${n}th`;
  return `${n}${["th", "st", "nd", "rd"][n % 10] ?? "th"}`;
}

/** "Every day at 09:00", "Weekdays at 07:30", "Once, Fri 14 Nov at 09:00", "Cron: 0 *\/2 * * *". */
export function describeSchedule(s: TaskSchedule): string {
  if (s.kind === "once") return `Once, ${formatWhen(s.at)}`;
  const f = fromSchedule(s);
  switch (f.frequency) {
    case "hourly":
      return f.minute === 0 ? "Every hour" : `Every hour at :${pad(f.minute)}`;
    case "daily":
      return `Every day at ${f.time}`;
    case "weekdays":
      return `Weekdays at ${f.time}`;
    case "weekly":
      return `Every ${WEEKDAY_NAMES[f.weekday]} at ${f.time}`;
    case "monthly":
      return `Monthly on the ${ordinal(f.dayOfMonth)} at ${f.time}`;
    default: {
      const macro: Record<string, string> = {
        "@hourly": "Every hour",
        "@daily": "Every day at 00:00",
        "@midnight": "Every day at 00:00",
        "@weekly": "Every Sunday at 00:00",
        "@monthly": "Monthly on the 1st at 00:00",
        "@yearly": "Every year on 1 January",
        "@annually": "Every year on 1 January",
      };
      return macro[s.expr.trim().toLowerCase()] ?? `Cron: ${s.expr.trim()}`;
    }
  }
}

/** "Fri 14 Nov, 09:00" (the year only when it isn't this year's). */
export function formatWhen(ts: number, now: Date = new Date()): string {
  const d = new Date(ts);
  const sameYear = d.getFullYear() === now.getFullYear();
  const date = d.toLocaleDateString(undefined, {
    weekday: "short",
    day: "numeric",
    month: "short",
    ...(sameYear ? {} : { year: "numeric" }),
  });
  return `${date}, ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

export const STATUS_LABEL: Record<RunStatus, string> = {
  running: "Running",
  waitingApproval: "Needs your approval",
  done: "Done",
  failed: "Failed",
  stopped: "Stopped",
  missed: "Missed",
  skipped: "Skipped",
};

export const TRIGGER_LABEL: Record<RunTrigger, string> = {
  schedule: "On schedule",
  manual: "Run now",
  catchUp: "Made up for a missed run",
};
