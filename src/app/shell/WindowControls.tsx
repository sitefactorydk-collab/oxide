import { useEffect, useState } from "react";
import {
  closeWindow,
  isWindowMaximized,
  minimizeWindow,
  onWindowResized,
  toggleMaximizeWindow,
} from "@/lib/window";
import { cn } from "@/lib/cn";

/** Windows-style caption buttons (minimize, maximize/restore, close), drawn with hairline glyphs. */
export function WindowControls() {
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    let alive = true;
    let unlisten: (() => void) | undefined;
    const refresh = () => {
      isWindowMaximized()
        .then((m) => alive && setMaximized(m))
        .catch(() => {});
    };
    refresh();
    onWindowResized(refresh)
      .then((u) => {
        if (alive) unlisten = u;
        else u();
      })
      .catch(() => {});
    return () => {
      alive = false;
      unlisten?.();
    };
  }, []);

  const base =
    "inline-flex h-full w-[46px] items-center justify-center text-fg transition-colors duration-100";
  return (
    <div className="flex h-full shrink-0 self-stretch">
      <button
        type="button"
        aria-label="Minimize"
        className={cn(base, "hover:bg-hover")}
        onClick={() => void minimizeWindow()}
      >
        <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden>
          <path d="M0 5.5h10" stroke="currentColor" strokeWidth="1" />
        </svg>
      </button>
      <button
        type="button"
        aria-label={maximized ? "Restore" : "Maximize"}
        className={cn(base, "hover:bg-hover")}
        onClick={() => void toggleMaximizeWindow()}
      >
        {maximized ? (
          <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden fill="none">
            <path d="M2.5 2.5V.5h7v7h-2" stroke="currentColor" strokeWidth="1" />
            <rect x=".5" y="2.5" width="7" height="7" stroke="currentColor" strokeWidth="1" />
          </svg>
        ) : (
          <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden fill="none">
            <rect x=".5" y=".5" width="9" height="9" stroke="currentColor" strokeWidth="1" />
          </svg>
        )}
      </button>
      <button
        type="button"
        aria-label="Close"
        className={cn(base, "hover:bg-winclose hover:text-winclose-fg")}
        onClick={() => void closeWindow()}
      >
        <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden>
          <path d="M.5.5l9 9M9.5.5l-9 9" stroke="currentColor" strokeWidth="1" />
        </svg>
      </button>
    </div>
  );
}
