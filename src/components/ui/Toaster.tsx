import { X } from "lucide-react";
import { useToasts } from "@/lib/toast";
import { cn } from "@/lib/cn";

/** Renders `toast()` messages. Mounted once in the app layout. */
export function Toaster() {
  const toasts = useToasts((s) => s.toasts);
  const dismiss = useToasts((s) => s.dismiss);
  return (
    <div
      role="status"
      aria-live="polite"
      className="pointer-events-none fixed bottom-6 left-1/2 z-50 flex -translate-x-1/2 flex-col items-center gap-2"
    >
      {toasts.map((t) => (
        <div
          key={t.id}
          className={cn(
            "ox-pop pointer-events-auto flex max-w-[min(560px,90vw)] items-center gap-3 rounded-xl border bg-raised py-2 pl-4 pr-2 text-base text-fg shadow-md",
            t.tone === "error" ? "border-danger" : "border-line",
          )}
        >
          <span className="min-w-0 flex-1 break-words">{t.text}</span>
          <button
            type="button"
            aria-label="Dismiss"
            onClick={() => dismiss(t.id)}
            className="inline-flex size-6 shrink-0 items-center justify-center rounded-md text-fg-subtle hover:bg-hover hover:text-fg"
          >
            <X className="size-3.5" />
          </button>
        </div>
      ))}
    </div>
  );
}
