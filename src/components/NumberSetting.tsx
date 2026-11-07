import { Check } from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";

/**
 * A number setting that saves on Enter, when the field loses focus, or with its Save button,
 * which appears (with an "Unsaved change" note) as soon as the value is edited. Values are
 * rounded and kept within `min`–`max`.
 */
export function NumberSetting({
  label,
  hint,
  value,
  min,
  max,
  step = 1,
  onChange,
}: {
  label: string;
  hint: ReactNode;
  value: number;
  min: number;
  max: number;
  step?: number;
  onChange: (n: number) => void;
}) {
  const [text, setText] = useState(String(value));
  useEffect(() => setText(String(value)), [value]);
  const dirty = text !== String(value);
  const commit = () => {
    const n = Math.round(Number(text));
    const next = text.trim() !== "" && Number.isFinite(n) ? Math.min(max, Math.max(min, n)) : value;
    setText(String(next));
    if (next !== value) onChange(next);
  };
  return (
    <div className="flex flex-col gap-2">
      <span className="text-md font-medium text-fg">{label}</span>
      <div className="flex items-center gap-2">
        <input
          type="number"
          inputMode="numeric"
          aria-label={label}
          min={min}
          max={max}
          step={step}
          value={text}
          onChange={(e) => setText(e.target.value)}
          onBlur={commit}
          onKeyDown={(e) => {
            if (e.key === "Enter") commit();
            if (e.key === "Escape") setText(String(value));
          }}
          className="h-10 w-40 rounded-lg border border-line bg-raised px-3 text-md text-fg outline-none focus:border-accent"
        />
        {dirty && (
          <>
            <button
              type="button"
              onClick={commit}
              className="inline-flex h-9 items-center gap-1.5 rounded-lg bg-fg px-3 text-base font-medium text-app hover:opacity-90"
            >
              <Check className="size-4" />
              Save
            </button>
            <span className="text-sm text-fg-subtle">Unsaved change · Enter saves, Esc undoes</span>
          </>
        )}
      </div>
      <span className="text-sm text-fg-subtle">{hint}</span>
    </div>
  );
}
