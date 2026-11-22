import { useEffect, useRef, useState } from "react";

/** How long one thought stays on the line before the next one replaces it. */
export const THOUGHT_HOLD_MS = 2000;

/**
 * The label to show: a new step shows at once, but a thought replacing another thought waits
 * until the shown one has had `THOUGHT_HOLD_MS`, so the line doesn't flicker while a model
 * thinks quickly. The newest thought wins when the wait is over.
 */
export function useSteadyLabel(label: string, thought: boolean): string {
  const [shown, setShown] = useState({ label, thought });
  const since = useRef(Date.now());
  useEffect(() => {
    if (label === shown.label) return;
    const show = () => {
      since.current = Date.now();
      setShown({ label, thought });
    };
    const wait = thought && shown.thought ? THOUGHT_HOLD_MS - (Date.now() - since.current) : 0;
    if (wait <= 0) {
      show();
      return;
    }
    const timer = setTimeout(show, wait);
    return () => clearTimeout(timer);
  }, [label, thought, shown]);
  return shown.label;
}
