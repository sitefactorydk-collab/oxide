import { CheckCircle2, KeyRound, Loader2, Plus, XCircle } from "lucide-react";
import { useId, useState, type ReactNode } from "react";
import type { ProviderConfig, ProviderKind, ProviderTestResult } from "@/bindings";
import { errorMessage } from "@/lib/api";
import { cn } from "@/lib/cn";
import { KIND_ORDER, PROVIDER_KINDS } from "./kinds";
import {
  useClearProviderKey,
  useDeleteProvider,
  useProviders,
  useSaveProvider,
  useSetProviderKey,
  useTestProvider,
} from "./queries";

/** Settings → Providers: endpoints, keys (OS keychain) and connection tests. */
export function ProvidersSection() {
  const { data: providers, isPending, error } = useProviders();
  const [adding, setAdding] = useState(false);

  if (isPending) return <p className="text-fg-muted">Loading…</p>;
  if (error) return <p className="text-danger">Couldn’t load providers: {errorMessage(error)}</p>;

  return (
    <div className="flex max-w-2xl flex-col gap-3">
      {providers.map((p) => (
        <ProviderCard key={p.id} provider={p} />
      ))}
      {adding ? (
        <div className="rounded-xl border border-line-strong bg-raised p-4 shadow-sm">
          <ProviderForm onDone={() => setAdding(false)} />
        </div>
      ) : (
        <button
          type="button"
          onClick={() => setAdding(true)}
          className="inline-flex h-10 items-center justify-center gap-2 rounded-xl border border-dashed border-line-strong text-base text-fg-muted transition-colors hover:bg-hover hover:text-fg"
        >
          <Plus className="size-4" />
          Add endpoint
        </button>
      )}
      <p className="mt-2 text-sm text-fg-subtle">
        API keys are stored in your system’s credential store, never in Oxide’s database or logs.
      </p>
    </div>
  );
}

function ProviderCard({ provider }: { provider: ProviderConfig }) {
  const [editing, setEditing] = useState(false);
  const meta = PROVIDER_KINDS[provider.kind];
  return (
    <section
      aria-label={provider.name}
      className={cn(
        "rounded-xl border bg-raised p-4 shadow-sm",
        editing ? "border-line-strong" : "border-line",
      )}
    >
      {editing ? (
        <ProviderForm provider={provider} onDone={() => setEditing(false)} />
      ) : (
        <div className="flex items-start gap-4">
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-2">
              <h3 className="text-md font-medium text-fg">{provider.name}</h3>
              {provider.name !== meta.label && (
                <span className="rounded-md bg-sunken px-1.5 py-0.5 text-xs text-fg-muted">
                  {meta.label}
                </span>
              )}
              {!provider.enabled && (
                <span className="rounded-md bg-sunken px-1.5 py-0.5 text-xs text-fg-subtle">
                  Off
                </span>
              )}
            </div>
            <p className="mt-0.5 truncate font-mono text-sm text-fg-muted">{provider.baseUrl}</p>
            <p className="mt-1.5 flex items-center gap-1.5 text-sm text-fg-subtle">
              <KeyRound className="size-3.5" />
              {provider.keyHint
                ? `Key saved (${provider.keyHint})`
                : meta.needsKey
                  ? "No API key yet"
                  : "No key needed for a local server"}
            </p>
          </div>
          <SecondaryButton onClick={() => setEditing(true)} label={`Edit ${provider.name}`}>
            Edit
          </SecondaryButton>
        </div>
      )}
    </section>
  );
}

/** Add (no `provider`) or edit an endpoint, including its key and a connection test. */
function ProviderForm({ provider, onDone }: { provider?: ProviderConfig; onDone: () => void }) {
  const formId = useId();
  const [kind, setKind] = useState<ProviderKind>(provider?.kind ?? "openai");
  const [name, setName] = useState(provider?.name ?? PROVIDER_KINDS.openai.label);
  const [baseUrl, setBaseUrl] = useState(provider?.baseUrl ?? PROVIDER_KINDS.openai.defaultUrl);
  const [enabled, setEnabled] = useState(provider?.enabled ?? true);
  const [key, setKey] = useState("");
  const [formError, setFormError] = useState<string | null>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);

  const save = useSaveProvider();
  const setProviderKey = useSetProviderKey();
  const clearKey = useClearProviderKey();
  const remove = useDeleteProvider();
  const test = useTestProvider();
  const busy = save.isPending || setProviderKey.isPending;

  const changeKind = (k: ProviderKind) => {
    // Follow the new kind's defaults unless the user already typed their own.
    const prev = PROVIDER_KINDS[kind];
    if (baseUrl === prev.defaultUrl) setBaseUrl(PROVIDER_KINDS[k].defaultUrl);
    if (name === prev.label) setName(PROVIDER_KINDS[k].label);
    setKind(k);
    test.reset();
  };

  const onSave = async () => {
    setFormError(null);
    try {
      const saved = await save.mutateAsync({
        id: provider?.id ?? null,
        kind,
        name,
        baseUrl,
        enabled,
      });
      if (key.trim()) await setProviderKey.mutateAsync({ id: saved.id, key });
      setKey("");
      onDone();
    } catch (e) {
      setFormError(errorMessage(e));
    }
  };

  const onTest = () =>
    test.mutate({
      kind,
      baseUrl,
      apiKey: key.trim() || null,
      providerId: provider?.id ?? null,
    });

  return (
    <form
      aria-label={provider ? `Edit ${provider.name}` : "New endpoint"}
      onSubmit={(e) => {
        e.preventDefault();
        void onSave();
      }}
      className="flex flex-col gap-4"
    >
      {!provider && (
        <FormRow label="Type" htmlFor={`${formId}-kind`}>
          <select
            id={`${formId}-kind`}
            value={kind}
            onChange={(e) => changeKind(e.target.value as ProviderKind)}
            className={inputClass}
          >
            {KIND_ORDER.map((k) => (
              <option key={k} value={k}>
                {PROVIDER_KINDS[k].label}
                {k === "openai" ? " (or any OpenAI-compatible API)" : ""}
              </option>
            ))}
          </select>
        </FormRow>
      )}
      <FormRow label="Name" htmlFor={`${formId}-name`}>
        <input
          id={`${formId}-name`}
          value={name}
          onChange={(e) => setName(e.target.value)}
          className={inputClass}
        />
      </FormRow>
      <FormRow
        label="Base URL"
        htmlFor={`${formId}-url`}
        aside={
          baseUrl !== PROVIDER_KINDS[kind].defaultUrl && (
            <button
              type="button"
              onClick={() => setBaseUrl(PROVIDER_KINDS[kind].defaultUrl)}
              className="text-sm text-accent hover:underline"
            >
              Use default
            </button>
          )
        }
      >
        <input
          id={`${formId}-url`}
          value={baseUrl}
          onChange={(e) => {
            setBaseUrl(e.target.value);
            test.reset();
          }}
          spellCheck={false}
          className={cn(inputClass, "font-mono text-base")}
        />
      </FormRow>
      <FormRow
        label="API key"
        htmlFor={`${formId}-key`}
        aside={
          provider?.keyHint && (
            <button
              type="button"
              onClick={() => clearKey.mutate(provider.id)}
              disabled={clearKey.isPending}
              className="text-sm text-danger hover:underline disabled:opacity-50"
            >
              Remove saved key
            </button>
          )
        }
      >
        <input
          id={`${formId}-key`}
          type="password"
          autoComplete="off"
          spellCheck={false}
          value={key}
          onChange={(e) => {
            setKey(e.target.value);
            test.reset();
          }}
          placeholder={
            provider?.keyHint
              ? `Saved (${provider.keyHint}). Type to replace it.`
              : PROVIDER_KINDS[kind].needsKey
                ? "Paste your API key"
                : "Optional for local servers"
          }
          className={cn(inputClass, "font-mono text-base")}
        />
      </FormRow>
      <label className="flex items-center gap-2 text-md text-fg">
        <input
          type="checkbox"
          checked={enabled}
          onChange={(e) => setEnabled(e.target.checked)}
          className="size-4 accent-[var(--ox-accent)]"
        />
        Show this endpoint’s models in the picker
      </label>

      {test.isPending && (
        <p className="flex items-center gap-2 text-base text-fg-muted" role="status">
          <Loader2 className="size-4 animate-spin" />
          Testing connection…
        </p>
      )}
      {test.data && <TestOutcome result={test.data} />}
      {test.error && <p className="text-base text-danger">{errorMessage(test.error)}</p>}
      {formError && (
        <p role="alert" className="text-base text-danger">
          {formError}
        </p>
      )}

      <div className="flex flex-wrap items-center gap-2">
        <button
          type="submit"
          disabled={busy}
          className="inline-flex h-9 items-center rounded-lg bg-fg px-3.5 text-base font-medium text-app transition-opacity hover:opacity-90 disabled:opacity-40"
        >
          {busy ? "Saving…" : "Save"}
        </button>
        <SecondaryButton onClick={onTest} disabled={test.isPending}>
          Test connection
        </SecondaryButton>
        <SecondaryButton onClick={onDone}>Cancel</SecondaryButton>
        {provider && (
          <span className="ml-auto">
            {confirmDelete ? (
              <span className="flex items-center gap-2 text-sm text-fg-muted">
                Delete this endpoint and its key?
                <button
                  type="button"
                  onClick={() => remove.mutate(provider.id, { onSuccess: onDone })}
                  className="h-8 rounded-lg bg-danger px-3 text-sm font-medium text-accent-fg"
                >
                  Delete
                </button>
                <button
                  type="button"
                  onClick={() => setConfirmDelete(false)}
                  className="h-8 rounded-lg px-2 text-sm text-fg-muted hover:bg-hover"
                >
                  Keep
                </button>
              </span>
            ) : (
              <button
                type="button"
                onClick={() => setConfirmDelete(true)}
                className="h-9 rounded-lg px-3 text-base text-danger hover:bg-danger-soft"
              >
                Delete
              </button>
            )}
          </span>
        )}
      </div>
    </form>
  );
}

function TestOutcome({ result }: { result: ProviderTestResult }) {
  return (
    <div
      role="status"
      className={cn(
        "rounded-lg px-3 py-2 text-base",
        result.ok ? "bg-sunken text-fg" : "bg-danger-soft text-fg",
      )}
    >
      <p className="flex items-center gap-2">
        {result.ok ? (
          <CheckCircle2 className="size-4 shrink-0 text-success" />
        ) : (
          <XCircle className="size-4 shrink-0 text-danger" />
        )}
        <span>{result.message}</span>
        <span className="ml-auto text-sm text-fg-subtle tabular-nums">{result.latencyMs} ms</span>
      </p>
      {result.models.length > 0 && (
        <p className="mt-1 truncate pl-6 font-mono text-sm text-fg-muted">
          {result.models.join(", ")}
          {result.modelCount > result.models.length ? ", …" : ""}
        </p>
      )}
    </div>
  );
}

const inputClass =
  "h-10 w-full rounded-lg border border-line bg-app px-3 text-md text-fg outline-none transition-colors focus:border-accent";

function FormRow({
  label,
  htmlFor,
  aside,
  children,
}: {
  label: string;
  htmlFor: string;
  aside?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div>
      <div className="mb-1.5 flex items-center justify-between">
        <label htmlFor={htmlFor} className="text-sm font-medium text-fg-muted">
          {label}
        </label>
        {aside}
      </div>
      {children}
    </div>
  );
}

function SecondaryButton({
  children,
  onClick,
  disabled,
  label,
}: {
  children: ReactNode;
  onClick: () => void;
  disabled?: boolean;
  label?: string;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      aria-label={label}
      className="inline-flex h-9 items-center rounded-lg border border-line px-3.5 text-base text-fg transition-colors hover:bg-hover disabled:opacity-40"
    >
      {children}
    </button>
  );
}
