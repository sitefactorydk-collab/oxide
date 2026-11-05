import { cn } from "@/lib/cn";

/**
 * The Oxide mark: an open ring (the "O") with one atom slipping out of the gap —
 * an oxygen bond breaking loose. Drawn in the accent colour; `tile` puts it on the app-icon tile.
 */
export function OxideMark({ className, title }: { className?: string; title?: string }) {
  return (
    <svg
      viewBox="0 0 32 32"
      className={cn("text-accent", className)}
      role={title ? "img" : undefined}
      aria-hidden={title ? undefined : true}
      aria-label={title}
    >
      <circle
        cx="15"
        cy="17"
        r="9"
        fill="none"
        stroke="currentColor"
        strokeWidth="4"
        strokeLinecap="round"
        strokeDasharray="40.5 16.05"
        transform="rotate(6.4 15 17)"
      />
      <circle cx="22.4" cy="9.6" r="2.6" fill="currentColor" />
    </svg>
  );
}
