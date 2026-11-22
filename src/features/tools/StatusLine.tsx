import { useEffect, useState } from "react";

/**
 * The pieces of a running reply's status line (Phase 24): a dot spinner, the reply's clock
 * and a label that fades in when it changes. Used by the running tool group, a thought
 * being written before any tool, and the plain “Working” line, so they look the same.
 */

/** Dots that blink in turn: a square of four for work, a row of three while thinking. */
export function Spinner({ thinking }: { thinking: boolean }) {
  const dot = "size-[4px] rounded-full bg-current";
  return thinking ? (
    <span
      aria-hidden
      className="ox-spin ox-spin-row inline-flex w-3.5 shrink-0 items-center justify-between text-accent"
    >
      <span className={dot} />
      <span className={dot} />
      <span className={dot} />
    </span>
  ) : (
    <span
      aria-hidden
      className="ox-spin grid size-3.5 shrink-0 grid-cols-2 place-items-center text-accent"
    >
      <span className={dot} />
      <span className={dot} />
      <span className={dot} />
      <span className={dot} />
    </span>
  );
}

/** “42s”, “12m 53s”. */
function clockText(secs: number): string {
  const s = Math.max(0, Math.floor(secs));
  return s < 60 ? `${s}s` : `${Math.floor(s / 60)}m ${s % 60}s`;
}

/** Time since the reply started, ticking every second. */
export function Clock({ since }: { since: number }) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, []);
  return (
    <span className="shrink-0 tabular-nums text-fg-subtle">{clockText((now - since) / 1000)}</span>
  );
}

/** The label, faded in each time it changes. */
export function StatusLabel({ label }: { label: string }) {
  return (
    <span key={label} className="ox-swap min-w-0 truncate">
      {label}
    </span>
  );
}
