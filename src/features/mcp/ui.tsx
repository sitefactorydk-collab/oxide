import type { ReactNode } from "react";
import type { McpStatus } from "@/bindings";
import { cn } from "@/lib/cn";

/** The coloured dot before a server's name. */
export function StatusDot({ status, className }: { status: McpStatus; className?: string }) {
  return (
    <span
      aria-hidden
      className={cn(
        "inline-block size-2 shrink-0 rounded-full",
        status === "running" && "bg-success",
        status === "starting" && "animate-pulse bg-accent",
        status === "error" && "bg-danger",
        status === "needsAuth" && "bg-accent",
        (status === "disabled" || status === "stopped") && "bg-line-strong",
        className,
      )}
    />
  );
}

export function Switch({
  checked,
  onChange,
  label,
  disabled,
  small,
}: {
  checked: boolean;
  onChange: (on: boolean) => void;
  label: string;
  disabled?: boolean;
  small?: boolean;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className={cn(
        "relative inline-flex shrink-0 items-center rounded-full transition-colors disabled:opacity-40",
        small ? "h-5 w-8" : "h-6 w-10",
        checked ? "bg-accent" : "bg-line-strong",
      )}
    >
      <span
        className={cn(
          "inline-block rounded-full bg-raised shadow-sm transition-transform",
          small ? "size-4" : "size-5",
          checked ? (small ? "translate-x-[14px]" : "translate-x-[18px]") : "translate-x-0.5",
        )}
      />
    </button>
  );
}

export const inputClass =
  "h-10 w-full rounded-lg border border-line bg-app px-3 text-md text-fg outline-none transition-colors focus:border-accent";

export const textareaClass =
  "w-full resize-y rounded-lg border border-line bg-app px-3 py-2 font-mono text-sm leading-5 text-fg outline-none transition-colors focus:border-accent";

export function Button({
  children,
  onClick,
  disabled,
  label,
  primary,
  danger,
  type = "button",
  className,
}: {
  children: ReactNode;
  onClick?: () => void;
  disabled?: boolean;
  label?: string;
  primary?: boolean;
  danger?: boolean;
  type?: "button" | "submit";
  className?: string;
}) {
  return (
    <button
      type={type}
      onClick={onClick}
      disabled={disabled}
      aria-label={label}
      className={cn(
        "inline-flex h-9 items-center gap-1.5 rounded-lg px-3.5 text-base transition-colors disabled:opacity-40 [&_svg]:size-4",
        primary
          ? "bg-accent text-accent-fg hover:bg-accent-hover"
          : danger
            ? "text-danger hover:bg-danger-soft"
            : "border border-line text-fg hover:bg-hover",
        className,
      )}
    >
      {children}
    </button>
  );
}

export function FormRow({
  label,
  htmlFor,
  hint,
  children,
}: {
  label: string;
  htmlFor: string;
  hint?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div>
      <label htmlFor={htmlFor} className="mb-1.5 block text-sm font-medium text-fg-muted">
        {label}
      </label>
      {children}
      {hint && <p className="mt-1 text-sm text-fg-subtle">{hint}</p>}
    </div>
  );
}
