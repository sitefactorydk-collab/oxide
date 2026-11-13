import { describe, expect, it } from "vitest";
import {
  defaultScheduleForm,
  describeSchedule,
  fromSchedule,
  toSchedule,
  type ScheduleForm,
} from "./schedule";

const NOW = new Date(2026, 10, 13, 8, 55); // Fri 13 Nov 2026, 08:55 local

function form(patch: Partial<ScheduleForm>): ScheduleForm {
  return { ...defaultScheduleForm(NOW), ...patch };
}

describe("schedule forms", () => {
  it("turn presets into cron and back", () => {
    const cases: Array<[Partial<ScheduleForm>, string]> = [
      [{ frequency: "daily", time: "09:00" }, "0 9 * * *"],
      [{ frequency: "weekdays", time: "07:30" }, "30 7 * * 1-5"],
      [{ frequency: "weekly", time: "18:05", weekday: 0 }, "5 18 * * 0"],
      [{ frequency: "monthly", time: "08:00", dayOfMonth: 15 }, "0 8 15 * *"],
      [{ frequency: "hourly", minute: 20 }, "20 * * * *"],
      [{ frequency: "custom", cron: "  0  */2 * * * " }, "0 */2 * * *"],
    ];
    for (const [patch, expr] of cases) {
      const s = toSchedule(form(patch));
      expect(s).toEqual({ kind: "cron", expr });
      const back = fromSchedule(s, NOW);
      expect(back.frequency).toBe(patch.frequency);
      expect(toSchedule(back)).toEqual(s);
    }
  });

  it("reads cron it can't show as custom", () => {
    expect(fromSchedule({ kind: "cron", expr: "0 9 * 1 *" }, NOW).frequency).toBe("custom");
    expect(fromSchedule({ kind: "cron", expr: "0 9 1 * 1" }, NOW).frequency).toBe("custom");
    expect(fromSchedule({ kind: "cron", expr: "@daily" }, NOW).frequency).toBe("custom");
  });

  it("makes one-time schedules from a local date and time", () => {
    const s = toSchedule(form({ frequency: "once", date: "2026-11-14", time: "09:30" }));
    expect(s).toEqual({ kind: "once", at: new Date(2026, 10, 14, 9, 30).getTime() });
    const back = fromSchedule(s, NOW);
    expect(back).toMatchObject({ frequency: "once", date: "2026-11-14", time: "09:30" });
    // A new task's one-time default is tomorrow.
    expect(defaultScheduleForm(NOW).date).toBe("2026-11-14");
  });

  it("describes schedules in words", () => {
    expect(describeSchedule({ kind: "cron", expr: "0 9 * * *" })).toBe("Every day at 09:00");
    expect(describeSchedule({ kind: "cron", expr: "30 7 * * 1-5" })).toBe("Weekdays at 07:30");
    expect(describeSchedule({ kind: "cron", expr: "0 9 * * 1" })).toBe("Every Monday at 09:00");
    expect(describeSchedule({ kind: "cron", expr: "0 8 2 * *" })).toBe(
      "Monthly on the 2nd at 08:00",
    );
    expect(describeSchedule({ kind: "cron", expr: "15 * * * *" })).toBe("Every hour at :15");
    expect(describeSchedule({ kind: "cron", expr: "@weekly" })).toBe("Every Sunday at 00:00");
    expect(describeSchedule({ kind: "cron", expr: "0 */2 * * *" })).toBe("Cron: 0 */2 * * *");
    expect(describeSchedule({ kind: "once", at: new Date(2026, 10, 14, 9, 0).getTime() })).toMatch(
      /^Once, .*14.*09:00$/,
    );
  });
});
