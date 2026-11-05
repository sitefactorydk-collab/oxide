import { cn } from "@/lib/cn";

/** Renders a shortcut like "Mod+Shift+O" as key caps, with Mod resolved per platform. */
export function Kbd({ keys, className }: { keys: string[]; className?: string }) {
  return (
    <span className={cn("inline-flex items-center gap-1", className)}>
      {keys.map((k) => (
        <kbd
          key={k}
          className="inline-flex h-6 min-w-6 items-center justify-center rounded-md border border-line bg-sunken px-1.5 font-sans text-xs font-medium text-fg-muted"
        >
          {k}
        </kbd>
      ))}
    </span>
  );
}
