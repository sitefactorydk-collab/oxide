import { useEffect, useState, type ReactNode } from "react";
import type { ModelInfo } from "@/bindings";
import { NumberSetting } from "@/components/NumberSetting";
import { DEFAULT_SETTINGS, useSetContextSettings, useSettings } from "@/features/settings/queries";
import { tokenCount } from "./queries";

const MIN_THRESHOLD = 30;
const MAX_THRESHOLD = 95;
const MIN_WINDOW = 2048;
const MAX_WINDOW = 10_000_000;

/** Settings → Models: automatic compaction, its threshold and model, Anthropic's clearing. */
export function ContextSettingsFields({
  models,
  modelLabel,
  Toggle,
}: {
  models: ModelInfo[];
  modelLabel: (m: ModelInfo) => string;
  Toggle: (p: {
    label: string;
    hint: string;
    checked: boolean;
    onChange: (on: boolean) => void;
  }) => ReactNode;
}) {
  const { data: settings } = useSettings();
  const context = (settings ?? DEFAULT_SETTINGS).context;
  const set = useSetContextSettings();
  return (
    <section aria-label="Long chats" className="flex flex-col gap-6 border-t border-line pt-6">
      <h3 className="text-md font-medium text-fg">Long chats</h3>
      <Toggle
        label="Compact long chats automatically"
        hint="When a chat fills most of the model’s context window, old tool results are cleared first, then the start of the chat is summarized. The transcript keeps everything; a divider shows where it happened."
        checked={context.compaction}
        onChange={(compaction) => set({ compaction })}
      />
      <NumberSetting
        label="Compact at (% of the context window)"
        hint={`Clearing and summarizing start above this share of the window. ${MIN_THRESHOLD}–${MAX_THRESHOLD}.`}
        value={context.threshold}
        min={MIN_THRESHOLD}
        max={MAX_THRESHOLD}
        onChange={(threshold) => set({ threshold })}
      />
      <label className="flex flex-col gap-2">
        <span className="text-md font-medium text-fg">Compaction model</span>
        <select
          value={context.compactionModel ?? ""}
          onChange={(e) => set({ compactionModel: e.target.value || null })}
          className="h-10 w-96 max-w-full rounded-lg border border-line bg-raised px-3 text-md text-fg outline-none focus:border-accent"
        >
          <option value="">The chat’s own model</option>
          {models.map((m) => (
            <option key={m.id} value={m.id}>
              {modelLabel(m)}
            </option>
          ))}
        </select>
        <span className="text-sm text-fg-subtle">
          Writes the summaries. Pick one with a large context window: long chats are summarized in
          parts when they don’t fit it at once.
        </span>
      </label>
      <Toggle
        label="Let Anthropic clear old tool results"
        hint="On Anthropic models, the API clears old tool results itself (context editing), which keeps its prompt cache working. Oxide still summarizes when that isn’t enough."
        checked={context.anthropicClearing}
        onChange={(anthropicClearing) => set({ anthropicClearing })}
      />
    </section>
  );
}

/**
 * A model's context window in the model list: what the provider reports, or the user's own
 * number (for servers that don't report one).
 */
export function WindowField({ model }: { model: ModelInfo }) {
  const { data: settings } = useSettings();
  const context = (settings ?? DEFAULT_SETTINGS).context;
  const set = useSetContextSettings();
  const override = context.windows.find((w) => w.model === model.id)?.tokens ?? null;
  const [editing, setEditing] = useState(false);
  const [text, setText] = useState("");
  useEffect(() => {
    if (editing) setText(String(override ?? model.contextWindow ?? ""));
  }, [editing, override, model.contextWindow]);

  const save = (tokens: number | null) => {
    const rest = context.windows.filter((w) => w.model !== model.id);
    set({ windows: tokens === null ? rest : [...rest, { model: model.id, tokens }] });
    setEditing(false);
  };

  if (editing) {
    return (
      <form
        className="flex items-center gap-1"
        onSubmit={(e) => {
          e.preventDefault();
          const n = Math.round(Number(text));
          if (Number.isFinite(n) && n > 0) {
            save(Math.min(MAX_WINDOW, Math.max(MIN_WINDOW, n)));
          }
        }}
      >
        <input
          type="number"
          autoFocus
          aria-label={`Context window of ${model.label}`}
          value={text}
          min={MIN_WINDOW}
          max={MAX_WINDOW}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Escape") setEditing(false);
          }}
          className="h-7 w-28 rounded-md border border-line bg-app px-2 text-sm outline-none focus:border-accent"
        />
        <button type="submit" className="h-7 rounded-md bg-fg px-2 text-xs font-medium text-app">
          Save
        </button>
        {override !== null && (
          <button
            type="button"
            onClick={() => save(null)}
            className="h-7 rounded-md px-2 text-xs text-fg-muted hover:bg-hover"
          >
            Reset
          </button>
        )}
      </form>
    );
  }
  const shown = override ?? model.contextWindow;
  return (
    <button
      type="button"
      onClick={() => setEditing(true)}
      title="Context window (click to set it)"
      className="rounded-md px-1.5 py-0.5 text-xs tabular-nums text-fg-subtle hover:bg-hover hover:text-fg"
    >
      {shown ? `${tokenCount(shown)} context` : "Context: unknown"}
      {override !== null && " (set)"}
    </button>
  );
}
