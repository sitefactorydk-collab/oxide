import { FolderOpen, Plus, X } from "lucide-react";
import { useId } from "react";
import type { ExtensionField, ExtensionValue } from "@/bindings";
import { Button, inputClass, Switch } from "@/features/mcp/ui";
import { errorMessage } from "@/lib/api";
import { toast } from "@/lib/toast";
import { pickPaths, type ExtensionValues } from "./queries";

/** The settings an extension's manifest asks for. */
export function ExtensionForm({
  fields,
  values,
  secretsSet,
  onChange,
}: {
  fields: ExtensionField[];
  values: ExtensionValues;
  secretsSet: string[];
  onChange: (values: ExtensionValues) => void;
}) {
  if (fields.length === 0) {
    return <p className="text-base text-fg-muted">This extension has no settings.</p>;
  }
  const set = (key: string, v: ExtensionValue) => onChange({ ...values, [key]: v });
  return (
    <div className="flex flex-col gap-4">
      {fields.map((f) => (
        <FieldInput
          key={f.key}
          field={f}
          value={values[f.key]}
          saved={f.sensitive && secretsSet.includes(f.key)}
          onChange={(v) => set(f.key, v)}
        />
      ))}
    </div>
  );
}

function FieldInput({
  field,
  value,
  saved,
  onChange,
}: {
  field: ExtensionField;
  value: ExtensionValue | undefined;
  saved: boolean;
  onChange: (v: ExtensionValue) => void;
}) {
  const id = useId();
  const label = (
    <label htmlFor={id} className="mb-1.5 block text-sm font-medium text-fg-muted">
      {field.title}
      {field.required && (
        <span className="text-danger" aria-hidden>
          {" "}
          *
        </span>
      )}
    </label>
  );
  const hint = field.description && (
    <p className="mt-1 text-sm text-fg-subtle">{field.description}</p>
  );

  if (field.type === "boolean") {
    return (
      <div className="flex items-start justify-between gap-6">
        <div>
          <div className="text-sm font-medium text-fg-muted">{field.title}</div>
          {hint}
        </div>
        <Switch
          label={field.title}
          checked={typeof value === "boolean" ? value : field.default === true}
          onChange={onChange}
        />
      </div>
    );
  }

  if (field.type === "number") {
    const shown =
      typeof value === "number" ? String(value) : typeof value === "string" ? value : "";
    const range =
      field.min !== null && field.max !== null
        ? `${field.min}–${field.max}`
        : field.min !== null
          ? `at least ${field.min}`
          : field.max !== null
            ? `at most ${field.max}`
            : null;
    return (
      <div>
        {label}
        <input
          id={id}
          type="number"
          inputMode="decimal"
          value={shown}
          min={field.min ?? undefined}
          max={field.max ?? undefined}
          placeholder={typeof field.default === "number" ? String(field.default) : undefined}
          onChange={(e) => {
            const t = e.target.value;
            const n = Number(t);
            onChange(t.trim() === "" || Number.isNaN(n) ? "" : n);
          }}
          className={`${inputClass} w-48`}
        />
        {range && <p className="mt-1 text-sm text-fg-subtle">{range}</p>}
        {hint}
      </div>
    );
  }

  if ((field.type === "directory" || field.type === "file") && field.multiple) {
    const list = Array.isArray(value) ? value : typeof value === "string" && value ? [value] : [];
    const add = async () => {
      try {
        const picked = await pickPaths(field.type === "directory" ? "folder" : "file", true);
        if (picked.length) onChange([...list, ...picked.filter((p) => !list.includes(p))]);
      } catch (e) {
        toast(errorMessage(e), "error");
      }
    };
    return (
      <div>
        {label}
        <ul aria-label={field.title} className="mb-2 flex flex-col gap-1">
          {list.map((p) => (
            <li
              key={p}
              className="flex items-center gap-2 rounded-lg border border-line bg-app px-3 py-1.5 text-base"
            >
              <span className="min-w-0 flex-1 truncate font-mono text-sm" title={p}>
                {p}
              </span>
              <button
                type="button"
                aria-label={`Remove ${p}`}
                onClick={() => onChange(list.filter((x) => x !== p))}
                className="rounded p-0.5 text-fg-subtle hover:bg-hover hover:text-fg"
              >
                <X className="size-3.5" />
              </button>
            </li>
          ))}
          {list.length === 0 && <li className="text-sm text-fg-subtle">None yet.</li>}
        </ul>
        <div className="flex gap-2">
          <Button onClick={() => void add()}>
            <Plus />
            {field.type === "directory" ? "Add folder" : "Add file"}
          </Button>
          <input
            id={id}
            aria-label={`Type a path for ${field.title}`}
            placeholder="or type a path and press Enter"
            className={`${inputClass} h-9 flex-1`}
            onKeyDown={(e) => {
              const t = e.currentTarget.value.trim();
              if (e.key === "Enter" && t) {
                e.preventDefault();
                if (!list.includes(t)) onChange([...list, t]);
                e.currentTarget.value = "";
              }
            }}
          />
        </div>
        {hint}
      </div>
    );
  }

  const text = typeof value === "string" ? value : Array.isArray(value) ? value.join(", ") : "";
  const browse =
    field.type === "directory" || field.type === "file"
      ? async () => {
          try {
            const [p] = await pickPaths(field.type === "directory" ? "folder" : "file");
            if (p) onChange(p);
          } catch (e) {
            toast(errorMessage(e), "error");
          }
        }
      : null;
  return (
    <div>
      {label}
      <div className="flex gap-2">
        <input
          id={id}
          type={field.sensitive ? "password" : "text"}
          autoComplete="off"
          spellCheck={false}
          value={text}
          placeholder={
            saved
              ? "Saved in the keychain. Type to replace it."
              : typeof field.default === "string"
                ? field.default
                : undefined
          }
          onChange={(e) => onChange(e.target.value)}
          className={`${inputClass} flex-1`}
        />
        {browse && (
          <Button onClick={() => void browse()} label={`Browse for ${field.title}`}>
            <FolderOpen />
            Browse
          </Button>
        )}
      </div>
      {hint}
    </div>
  );
}
