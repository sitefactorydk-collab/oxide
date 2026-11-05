import { useEffect, useRef, useState } from "react";
import { cn } from "@/lib/cn";

/** Single-line rename field: Enter or blur commits, Escape cancels. */
export function InlineRename({
  value,
  onCommit,
  onCancel,
  className,
}: {
  value: string;
  onCommit: (next: string) => void;
  onCancel: () => void;
  className?: string;
}) {
  const [text, setText] = useState(value);
  const ref = useRef<HTMLInputElement>(null);
  const done = useRef(false);

  useEffect(() => {
    ref.current?.focus();
    ref.current?.select();
  }, []);

  const commit = () => {
    if (done.current) return;
    done.current = true;
    const t = text.trim();
    if (t && t !== value) onCommit(t);
    else onCancel();
  };

  return (
    <input
      ref={ref}
      aria-label="Chat title"
      value={text}
      onChange={(e) => setText(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => {
        if (e.key === "Enter") {
          e.preventDefault();
          commit();
        } else if (e.key === "Escape") {
          e.preventDefault();
          done.current = true;
          onCancel();
        }
      }}
      className={cn(
        "min-w-0 rounded-md border border-line-strong bg-raised px-2 text-base text-fg outline-none focus:border-accent",
        className,
      )}
    />
  );
}
