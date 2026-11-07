import type { TaskItem } from "@/bindings";

/** Steps done, steps in all, and the step being worked on (index, or `null`). */
export function taskProgress(items: TaskItem[]): {
  done: number;
  total: number;
  current: number | null;
} {
  const done = items.filter((t) => t.status === "done").length;
  let current = items.findIndex((t) => t.status === "inProgress");
  if (current < 0) current = items.findIndex((t) => t.status !== "done");
  return { done, total: items.length, current: current < 0 ? null : current };
}

/** The title bar pill: "Step 2 of 5", or "5 of 5 done" when finished. */
export function stepLabel(items: TaskItem[]): string | null {
  const p = taskProgress(items);
  if (p.total === 0) return null;
  if (p.current === null) return `${p.done} of ${p.total} done`;
  return `Step ${p.current + 1} of ${p.total}`;
}
