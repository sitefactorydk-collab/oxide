import { Loader2, RefreshCw } from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";
import type { ModelInfo, ProviderModels } from "@/bindings";
import { useSetModelSettings, useSettings, DEFAULT_SETTINGS } from "@/features/settings/queries";
import { errorMessage } from "@/lib/api";
import { cn } from "@/lib/cn";
import { allModels, isUsable } from "./models";
import { useModels, useRefreshModels } from "./queries";

const MIN_BUDGET = 1024;
const MAX_BUDGET = 64000;

/** Settings → Models: picker options, titles, thinking budget, and every endpoint's models. */
export function ModelsSection() {
  const { data: settings } = useSettings();
  const models = (settings ?? DEFAULT_SETTINGS).models;
  const setModels = useSetModelSettings();
  const { data: lists, isPending, error } = useModels();
  const refresh = useRefreshModels();
  const usable = allModels(lists).filter(isUsable);

  return (
    <div className="flex max-w-2xl flex-col gap-6">
      <Toggle
        label="Show models without tool calling"
        hint="They appear greyed out in the model picker. Oxide’s tools need native tool calling, so they can’t be picked."
        checked={models.showUnsupported}
        onChange={(showUnsupported) => setModels({ showUnsupported })}
      />
      <Toggle
        label="Name chats automatically"
        hint="After the first reply, a small model suggests a short title. Chats you renamed keep their name."
        checked={models.autoTitle}
        onChange={(autoTitle) => setModels({ autoTitle })}
      />
      <label className="flex flex-col gap-2">
        <span className="text-md font-medium text-fg">Title model</span>
        <select
          value={models.titleModel ?? ""}
          disabled={!models.autoTitle}
          onChange={(e) => setModels({ titleModel: e.target.value || null })}
          className="h-10 w-96 max-w-full rounded-lg border border-line bg-raised px-3 text-md text-fg outline-none focus:border-accent disabled:opacity-50"
        >
          <option value="">Automatic (a small model from the chat’s provider)</option>
          {usable.map((m) => (
            <option key={m.id} value={m.id}>
              {m.label} ({providerName(lists, m)})
            </option>
          ))}
        </select>
      </label>
      <BudgetField
        value={models.thinkingBudget}
        onChange={(thinkingBudget) => setModels({ thinkingBudget })}
      />

      <div className="flex items-center justify-between border-t border-line pt-6">
        <h3 className="text-md font-medium text-fg">Available models</h3>
        <button
          type="button"
          onClick={() => refresh.mutate()}
          disabled={refresh.isPending}
          className="inline-flex h-8 items-center gap-2 rounded-lg border border-line px-3 text-sm text-fg-muted hover:bg-hover hover:text-fg disabled:opacity-60"
        >
          {refresh.isPending ? (
            <Loader2 className="size-4 animate-spin" />
          ) : (
            <RefreshCw className="size-4" />
          )}
          Refresh
        </button>
      </div>
      {refresh.error && <p className="text-danger">{errorMessage(refresh.error)}</p>}
      {isPending && <p className="text-fg-muted">Loading models…</p>}
      {error && <p className="text-danger">Couldn’t list models: {errorMessage(error)}</p>}
      {lists.map((list) => (
        <ProviderModelList key={list.providerId} list={list} />
      ))}
      <p className="text-sm text-fg-subtle">
        Refresh also re-checks tool support for servers that don’t report it (Strata), with one tiny
        request per model.
      </p>
    </div>
  );
}

function providerName(lists: ProviderModels[], m: ModelInfo): string {
  return lists.find((l) => l.providerId === m.providerId)?.providerName ?? m.providerId;
}

function ProviderModelList({ list }: { list: ProviderModels }) {
  const usable = list.models.filter(isUsable).length;
  return (
    <section aria-label={`${list.providerName} models`} className="flex flex-col gap-2">
      <div className="flex items-baseline justify-between">
        <h4 className="text-base font-medium text-fg">{list.providerName}</h4>
        {list.models.length > 0 && (
          <span className="text-sm text-fg-subtle">
            {usable} of {list.models.length} with tool calling
          </span>
        )}
      </div>
      {list.error && <p className="text-sm text-fg-muted">{list.error}</p>}
      {list.models.length > 0 && (
        <ul className="divide-y divide-line rounded-xl border border-line bg-raised">
          {list.models.map((m) => (
            <li
              key={m.id}
              className={cn("flex items-center gap-3 px-3 py-2", !isUsable(m) && "opacity-55")}
            >
              <div className="min-w-0 flex-1">
                <div className="truncate text-base text-fg">{m.label}</div>
                {m.label !== m.model && (
                  <div className="truncate font-mono text-xs text-fg-subtle">{m.model}</div>
                )}
              </div>
              <div className="flex shrink-0 gap-1">
                <Badge on={m.capabilities.tools}>
                  {m.capabilities.tools ? "Tools" : "No tools"}
                </Badge>
                {m.capabilities.thinking !== "none" && <Badge on>Thinking</Badge>}
                {m.capabilities.images && <Badge on>Images</Badge>}
                {m.capabilities.pdfs && <Badge on>PDFs</Badge>}
              </div>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

function Badge({ on, children }: { on: boolean; children: ReactNode }) {
  return (
    <span
      className={cn(
        "rounded-md px-1.5 py-0.5 text-xs",
        on ? "bg-accent-soft text-accent" : "bg-sunken text-fg-subtle",
      )}
    >
      {children}
    </span>
  );
}

function Toggle({
  label,
  hint,
  checked,
  onChange,
}: {
  label: string;
  hint: string;
  checked: boolean;
  onChange: (on: boolean) => void;
}) {
  return (
    <div className="flex items-start justify-between gap-6">
      <div>
        <div className="text-md font-medium text-fg">{label}</div>
        <p className="mt-1 text-sm text-fg-subtle">{hint}</p>
      </div>
      <button
        type="button"
        role="switch"
        aria-checked={checked}
        aria-label={label}
        onClick={() => onChange(!checked)}
        className={cn(
          "relative mt-0.5 inline-flex h-6 w-10 shrink-0 items-center rounded-full transition-colors",
          checked ? "bg-accent" : "bg-line-strong",
        )}
      >
        <span
          className={cn(
            "inline-block size-5 rounded-full bg-raised shadow-sm transition-transform",
            checked ? "translate-x-[18px]" : "translate-x-0.5",
          )}
        />
      </button>
    </div>
  );
}

/** Number field that saves a clamped value when the user leaves it. */
function BudgetField({ value, onChange }: { value: number; onChange: (n: number) => void }) {
  const [text, setText] = useState(String(value));
  useEffect(() => setText(String(value)), [value]);
  const commit = () => {
    const n = Math.round(Number(text));
    const next = Number.isFinite(n) ? Math.min(MAX_BUDGET, Math.max(MIN_BUDGET, n)) : value;
    setText(String(next));
    if (next !== value) onChange(next);
  };
  return (
    <label className="flex flex-col gap-2">
      <span className="text-md font-medium text-fg">Thinking budget (tokens)</span>
      <input
        type="number"
        inputMode="numeric"
        min={MIN_BUDGET}
        max={MAX_BUDGET}
        step={1024}
        value={text}
        onChange={(e) => setText(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter") commit();
        }}
        className="h-10 w-40 rounded-lg border border-line bg-raised px-3 text-md text-fg outline-none focus:border-accent"
      />
      <span className="text-sm text-fg-subtle">
        For models that take a budget when extended thinking is on. Models that decide for
        themselves ignore it. {MIN_BUDGET.toLocaleString("en")}–{MAX_BUDGET.toLocaleString("en")}.
      </span>
    </label>
  );
}
