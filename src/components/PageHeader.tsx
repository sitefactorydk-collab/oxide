import type { ReactNode } from "react";

/** Serif page title with an optional action on the right. */
export function PageHeader({ title, action }: { title: string; action?: ReactNode }) {
  return (
    <div className="mb-6 flex items-center justify-between gap-4">
      <h1 className="font-serif text-2xl font-normal tracking-tight text-fg">{title}</h1>
      {action}
    </div>
  );
}

export function PrimaryButton({
  children,
  onClick,
  disabled,
  title,
}: {
  children: ReactNode;
  onClick?: () => void;
  disabled?: boolean;
  title?: string;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      title={title}
      className="inline-flex h-9 items-center gap-2 rounded-lg bg-fg px-3.5 text-base font-medium text-app transition-opacity hover:opacity-90 disabled:opacity-40 [&_svg]:size-4"
    >
      {children}
    </button>
  );
}

export function SearchField({
  value,
  onChange,
  placeholder,
  label,
}: {
  value: string;
  onChange: (v: string) => void;
  placeholder: string;
  label: string;
}) {
  return (
    <input
      type="search"
      aria-label={label}
      value={value}
      onChange={(e) => onChange(e.target.value)}
      placeholder={placeholder}
      className="mb-5 h-10 w-full rounded-xl border border-line bg-raised px-3.5 text-md text-fg outline-none transition-colors focus:border-line-strong"
    />
  );
}
