import { Check, MessageSquare, Pencil, Plus, Trash2, X } from "lucide-react";
import { useEffect, useState } from "react";
import { Link } from "react-router";
import type { MemoryItem } from "@/bindings";
import { IconButton } from "@/components/ui/IconButton";
import { allModels, isUsable } from "@/features/models/models";
import { useModels } from "@/features/models/queries";
import { Toggle } from "@/features/models/ModelsSection";
import { ConfirmDialog } from "@/features/projects/ProjectDialogs";
import { useProjects } from "@/features/projects/queries";
import {
  DEFAULT_SETTINGS,
  useSetMemorySettings,
  useSetProjectSettings,
  useSettings,
} from "@/features/settings/queries";
import { errorMessage } from "@/lib/api";
import {
  groupMemory,
  useAddMemory,
  useClearMemory,
  useDeleteMemory,
  useMemory,
  useUpdateMemory,
} from "./queries";

const MIN_BUDGET = 1000;
const MAX_BUDGET = 400000;

const input =
  "h-9 min-w-0 flex-1 rounded-lg border border-line bg-raised px-3 text-base text-fg outline-none focus:border-accent";

/** Settings → Memory: memory on/off, the memory model, every item, and the knowledge budget. */
export function MemorySection() {
  const { data } = useSettings();
  const settings = data ?? DEFAULT_SETTINGS;
  const setMemory = useSetMemorySettings();
  const setProjects = useSetProjectSettings();
  const { data: lists } = useModels();
  const usable = allModels(lists).filter(isUsable);

  return (
    <div className="flex max-w-2xl flex-col gap-6">
      <Toggle
        label="Remember things across chats"
        hint="After each reply a small model notes lasting facts about you (your name, preferences, ongoing work) and later chats get them. Items learned in a project stay in that project. Off by default; nothing leaves your computer except to the model you chose."
        checked={settings.memory.enabled}
        onChange={(enabled) => setMemory({ enabled })}
      />
      <label className="flex flex-col gap-2">
        <span className="text-md font-medium text-fg">Memory model</span>
        <select
          value={settings.memory.model ?? ""}
          disabled={!settings.memory.enabled}
          onChange={(e) => setMemory({ model: e.target.value || null })}
          className="h-10 w-96 max-w-full rounded-lg border border-line bg-raised px-3 text-md text-fg outline-none focus:border-accent disabled:opacity-50"
        >
          <option value="">Automatic (a small model from the chat’s provider)</option>
          {usable.map((m) => (
            <option key={m.id} value={m.id}>
              {m.label}
            </option>
          ))}
        </select>
      </label>

      <MemoryList enabled={settings.memory.enabled} />

      <div className="border-t border-line pt-6">
        <h3 className="mb-1 text-md font-medium text-fg">Project knowledge</h3>
        <BudgetField
          value={settings.projects.knowledgeBudget}
          onChange={(knowledgeBudget) => setProjects({ knowledgeBudget })}
        />
      </div>
    </div>
  );
}

function MemoryList({ enabled }: { enabled: boolean }) {
  const { data: items, error } = useMemory();
  const { data: projects } = useProjects();
  const clear = useClearMemory();
  const [confirming, setConfirming] = useState(false);
  const groups = groupMemory(items, (id) => projects.find((p) => p.id === id)?.name);

  return (
    <section aria-label="Saved memory" className="flex flex-col gap-4">
      <div className="flex items-center justify-between">
        <h3 className="text-md font-medium text-fg">
          What Oxide remembers <span className="font-normal text-fg-subtle">({items.length})</span>
        </h3>
        {items.length > 0 && (
          <button
            type="button"
            onClick={() => setConfirming(true)}
            className="h-8 rounded-lg px-3 text-sm text-danger hover:bg-danger-soft"
          >
            Delete all
          </button>
        )}
      </div>
      {!enabled && items.length > 0 && (
        <p className="text-sm text-fg-subtle">
          Memory is off: these items are kept but not given to models.
        </p>
      )}
      {error && <p className="text-base text-danger">{errorMessage(error)}</p>}
      {groups.map((g) =>
        g.items.length === 0 && g.projectId !== null ? null : (
          <div key={g.key}>
            <h4 className="mb-1 text-sm font-medium text-fg-subtle">{g.title}</h4>
            <ul className="divide-y divide-line rounded-xl border border-line bg-raised">
              {g.items.map((m) => (
                <MemoryRow key={m.id} item={m} />
              ))}
              {g.items.length === 0 && (
                <li className="px-3 py-3 text-base text-fg-subtle">Nothing yet.</li>
              )}
            </ul>
          </div>
        ),
      )}
      <AddMemory projects={projects.filter((p) => !p.archived)} />
      <ConfirmDialog
        open={confirming}
        onOpenChange={setConfirming}
        title="Delete all memory?"
        body="Every item Oxide remembers, in all chats and projects, is deleted. This can’t be undone."
        confirmLabel="Delete all"
        onConfirm={() => clear.mutate(null)}
      />
    </section>
  );
}

function MemoryRow({ item }: { item: MemoryItem }) {
  const update = useUpdateMemory();
  const remove = useDeleteMemory();
  const [editing, setEditing] = useState(false);
  const [text, setText] = useState(item.text);
  useEffect(() => setText(item.text), [item.text]);

  if (editing) {
    return (
      <li className="flex items-center gap-2 px-3 py-2">
        <form
          className="flex flex-1 items-center gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            update.mutate({ id: item.id, text }, { onSuccess: () => setEditing(false) });
          }}
        >
          <input
            autoFocus
            aria-label="Memory text"
            value={text}
            onChange={(e) => setText(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape") {
                setText(item.text);
                setEditing(false);
              }
            }}
            className={input}
          />
          <IconButton size="sm" type="submit" label="Save" disabled={!text.trim()}>
            <Check className="size-4" />
          </IconButton>
          <IconButton
            size="sm"
            label="Cancel"
            onClick={() => {
              setText(item.text);
              setEditing(false);
            }}
          >
            <X className="size-4" />
          </IconButton>
        </form>
      </li>
    );
  }
  return (
    <li className="group flex items-center gap-2 px-3 py-2">
      <span className="min-w-0 flex-1 text-base text-fg">{item.text}</span>
      {item.conversationId && (
        <Link
          to={`/chat/${item.conversationId}`}
          aria-label="The chat it came from"
          title="The chat it came from"
          className="text-fg-subtle hover:text-fg"
        >
          <MessageSquare className="size-3.5" />
        </Link>
      )}
      <IconButton size="sm" label={`Edit “${item.text}”`} onClick={() => setEditing(true)}>
        <Pencil className="size-3.5" />
      </IconButton>
      <IconButton size="sm" label={`Delete “${item.text}”`} onClick={() => remove.mutate(item.id)}>
        <Trash2 className="size-3.5" />
      </IconButton>
    </li>
  );
}

function AddMemory({ projects }: { projects: Array<{ id: string; name: string }> }) {
  const add = useAddMemory();
  const [text, setText] = useState("");
  const [scope, setScope] = useState("");
  return (
    <form
      className="flex flex-col gap-2"
      onSubmit={(e) => {
        e.preventDefault();
        add.mutate({ text, projectId: scope || null }, { onSuccess: () => setText("") });
      }}
    >
      <div className="flex items-center gap-2">
        <input
          aria-label="New memory"
          value={text}
          onChange={(e) => setText(e.target.value)}
          placeholder="Add something for Oxide to remember…"
          className={input}
        />
        <select
          aria-label="Where it applies"
          value={scope}
          onChange={(e) => setScope(e.target.value)}
          className="h-9 max-w-48 rounded-lg border border-line bg-raised px-2 text-base text-fg outline-none focus:border-accent"
        >
          <option value="">All chats</option>
          {projects.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
            </option>
          ))}
        </select>
        <button
          type="submit"
          disabled={!text.trim() || add.isPending}
          className="inline-flex h-9 items-center gap-1.5 rounded-lg bg-fg px-3 text-base font-medium text-app hover:opacity-90 disabled:opacity-40"
        >
          <Plus className="size-4" />
          Add
        </button>
      </div>
      {add.error && <p className="text-sm text-danger">{errorMessage(add.error)}</p>}
    </form>
  );
}

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
      <span className="text-sm text-fg-muted">Knowledge budget (tokens)</span>
      <input
        type="number"
        inputMode="numeric"
        min={MIN_BUDGET}
        max={MAX_BUDGET}
        step={1000}
        value={text}
        onChange={(e) => setText(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter") commit();
        }}
        className="h-10 w-40 rounded-lg border border-line bg-raised px-3 text-md text-fg outline-none focus:border-accent"
      />
      <span className="text-sm text-fg-subtle">
        A project whose files fit this many tokens goes into every chat in full (cached on
        Anthropic). Larger projects are searched instead. A chat also keeps it under a quarter of
        the model’s context window.
      </span>
    </label>
  );
}
