import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ChevronRight, Film, ImageIcon, Music, type LucideIcon } from "lucide-react";
import { useEffect, useState } from "react";
import {
  commands,
  type GenerationEndpoint,
  type GenerationKind,
  type GenerationTest,
} from "@/bindings";
import { useProviders } from "@/features/providers/queries";
import {
  DEFAULT_SETTINGS,
  useSetGenerationSettings,
  useSettings,
} from "@/features/settings/queries";
import { errorMessage, unwrap } from "@/lib/api";
import { cn } from "@/lib/cn";

const generationKeys = {
  keys: ["generation", "keys"] as const,
  models: (kind: GenerationKind, ep: GenerationEndpoint) =>
    ["generation", "models", kind, ep.providerId, ep.baseUrl] as const,
};

/** Models that usually fit each kind (typed models work too). */
const SUGGESTED: Record<GenerationKind, string[]> = {
  image: [
    "gpt-image-2",
    "gpt-image-1.5",
    "gpt-image-1-mini",
    "dall-e-3",
    "imagen-4.0-generate-001",
  ],
  video: ["sora-2", "sora-2-pro"],
  music: ["ace-step"],
};

const GROUPS: { kind: GenerationKind; title: string; icon: LucideIcon; hint: string }[] = [
  {
    kind: "image",
    title: "Image generation",
    icon: ImageIcon,
    hint: "Used by generate_image in chats: /images/generations, and /images/edits when it starts from images in the chat.",
  },
  {
    kind: "video",
    title: "Video generation",
    icon: Film,
    hint: "Used by generate_video in chats. Works with the /videos job API, or with /videos/generations that answers with the finished file.",
  },
  {
    kind: "music",
    title: "Music generation",
    icon: Music,
    hint: "Used by generate_music in chats for songs and instrumentals: POST /music/generations (one request, one finished file; it may take minutes when the server is busy).",
  },
];

const input =
  "h-9 w-full rounded-lg border border-line bg-raised px-3 text-md text-fg outline-none focus:border-accent";
const button =
  "inline-flex h-9 shrink-0 items-center rounded-lg border border-line bg-raised px-3 text-md text-fg-muted hover:bg-hover hover:text-fg disabled:opacity-50";

function useGenerationKeys() {
  return useQuery({
    queryKey: generationKeys.keys,
    queryFn: () => unwrap(commands.generationKeys()),
  });
}

/** Settings → Media generation: an endpoint and model per kind of generation. */
export function GenerationSection() {
  const { data } = useSettings();
  const g = (data ?? DEFAULT_SETTINGS).generation;
  const [open, setOpen] = useState<GenerationKind | null>("image");
  return (
    <div className="flex max-w-2xl flex-col gap-3">
      <p className="text-sm text-fg-subtle">
        Every endpoint here speaks the OpenAI-compatible API: OpenAI, Gemini’s and Groq’s compatible
        endpoints, LM Studio, LocalAI, vLLM and other servers. Pick one from Settings → Providers or
        give a URL. Generating costs money, so each chat asks before it runs a generation tool. Text
        comes from the chat’s own model (Providers and Models).
      </p>
      {GROUPS.map((group) => (
        <GroupCard
          key={group.kind}
          group={group}
          endpoint={g[group.kind]}
          open={open === group.kind}
          onToggle={() => setOpen((o) => (o === group.kind ? null : group.kind))}
        />
      ))}
    </div>
  );
}

function GroupCard({
  group,
  endpoint,
  open,
  onToggle,
}: {
  group: (typeof GROUPS)[number];
  endpoint: GenerationEndpoint;
  open: boolean;
  onToggle: () => void;
}) {
  const Icon = group.icon;
  const ready = endpoint.model !== "" && (endpoint.providerId !== null || endpoint.baseUrl !== "");
  return (
    <section
      aria-label={group.title}
      className="overflow-hidden rounded-xl border border-line bg-raised"
    >
      <button
        type="button"
        aria-expanded={open}
        onClick={onToggle}
        className="flex w-full items-center gap-3 px-4 py-3 text-left hover:bg-hover"
      >
        <Icon aria-hidden className="size-4 shrink-0 text-fg-muted" />
        <span className="text-md font-medium text-fg">{group.title}</span>
        <span
          className={cn(
            "ml-auto truncate rounded-md px-2 py-0.5 font-mono text-xs",
            ready ? "bg-sunken text-fg-muted" : "text-fg-subtle",
          )}
        >
          {ready ? endpoint.model : "Not set up"}
        </span>
        <ChevronRight
          aria-hidden
          className={cn("size-4 shrink-0 text-fg-subtle transition-transform", open && "rotate-90")}
        />
      </button>
      {open && <GroupForm kind={group.kind} hint={group.hint} endpoint={endpoint} />}
    </section>
  );
}

function GroupForm({
  kind,
  hint,
  endpoint,
}: {
  kind: GenerationKind;
  hint: string;
  endpoint: GenerationEndpoint;
}) {
  const set = useSetGenerationSettings();
  const { data: providers } = useProviders();
  const usable = providers.filter((p) => p.kind !== "anthropic");
  const save = (patch: Partial<GenerationEndpoint>) => set({ [kind]: { ...endpoint, ...patch } });

  const [model, setModel] = useState(endpoint.model);
  const [url, setUrl] = useState(endpoint.baseUrl);
  useEffect(() => setModel(endpoint.model), [endpoint.model]);
  useEffect(() => setUrl(endpoint.baseUrl), [endpoint.baseUrl]);

  const reachable = endpoint.providerId !== null || endpoint.baseUrl !== "";
  const [listing, setListing] = useState(false);
  const models = useQuery({
    queryKey: generationKeys.models(kind, endpoint),
    queryFn: () => unwrap(commands.generationModels(kind, endpoint)),
    enabled: listing && reachable,
  });
  const suggestions = [...new Set([...(models.data ?? []), ...SUGGESTED[kind]])];

  const [test, setTest] = useState<GenerationTest | null>(null);
  const runTest = useMutation({
    mutationFn: () =>
      unwrap(
        commands.testGeneration(kind, { ...endpoint, model: model.trim(), baseUrl: url.trim() }),
      ),
    onSuccess: setTest,
    onError: (e) => setTest({ ok: false, message: errorMessage(e) }),
  });

  const listId = `generation-models-${kind}`;
  return (
    <div className="flex flex-col gap-5 border-t border-line px-4 py-4">
      <p className="text-sm text-fg-subtle">{hint}</p>
      <label className="flex flex-col gap-2">
        <span className="text-md font-medium text-fg">Endpoint</span>
        <select
          aria-label={`${kind} endpoint`}
          value={endpoint.providerId ?? ""}
          onChange={(e) => save({ providerId: e.target.value || null })}
          className={input}
        >
          <option value="">{url.trim() ? "Custom URL (below)" : "Not set up"}</option>
          {usable.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
            </option>
          ))}
        </select>
        <span className="text-sm text-fg-subtle">
          An endpoint from Settings → Providers; its URL and key are used.
        </span>
      </label>
      <label className="flex flex-col gap-2">
        <span className="text-md font-medium text-fg">Custom URL (optional)</span>
        <input
          aria-label={`${kind} custom URL`}
          placeholder="https://…/v1"
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          onBlur={() => url.trim() !== endpoint.baseUrl && save({ baseUrl: url.trim() })}
          className={input}
        />
        <span className="text-sm text-fg-subtle">
          An OpenAI-compatible base URL ending in <code>/v1</code>. It takes the place of the
          endpoint’s URL.
        </span>
      </label>
      <div className="flex flex-col gap-2">
        <label htmlFor={`generation-model-${kind}`} className="text-md font-medium text-fg">
          Model
        </label>
        <div className="flex gap-2">
          <input
            id={`generation-model-${kind}`}
            aria-label={`${kind} model`}
            list={listId}
            value={model}
            onChange={(e) => setModel(e.target.value)}
            onBlur={() => model.trim() !== endpoint.model && save({ model: model.trim() })}
            className={input}
          />
          <button
            type="button"
            className={button}
            disabled={!reachable || models.isFetching}
            onClick={() => {
              setListing(true);
              if (listing) void models.refetch();
            }}
          >
            {models.isFetching ? "Loading…" : "List models"}
          </button>
        </div>
        <datalist id={listId}>
          {suggestions.map((m) => (
            <option key={m} value={m} />
          ))}
        </datalist>
        <span className="text-sm text-fg-subtle">
          {models.isError
            ? `The models couldn’t be listed: ${errorMessage(models.error)}`
            : models.data
              ? `The endpoint lists ${models.data.length} models; pick one or type a name.`
              : "Pick a suggestion, list the endpoint’s models, or type a name."}
        </span>
      </div>
      <KeyField kind={kind} />
      <div className="flex items-center gap-3">
        <button
          type="button"
          className={button}
          disabled={runTest.isPending}
          onClick={() => {
            setTest(null);
            runTest.mutate();
          }}
        >
          {runTest.isPending ? "Testing…" : "Test"}
        </button>
        {test && (
          <span role="status" className={cn("text-sm", test.ok ? "text-fg-muted" : "text-danger")}>
            {test.message}
          </span>
        )}
      </div>
    </div>
  );
}

/** A key of this kind's own, kept in the keychain. */
function KeyField({ kind }: { kind: GenerationKind }) {
  const qc = useQueryClient();
  const keys = useGenerationKeys();
  const saved = keys.data?.includes(kind) ?? false;
  const [key, setKey] = useState("");
  const setSaved = useMutation({
    mutationFn: (value: string | null) => unwrap(commands.setGenerationKey(kind, value)),
    onSuccess: () => {
      setKey("");
      void qc.invalidateQueries({ queryKey: generationKeys.keys });
      void qc.invalidateQueries({ queryKey: ["generation", "models", kind] });
    },
  });
  return (
    <div className="flex flex-col gap-2">
      <label htmlFor={`generation-key-${kind}`} className="text-md font-medium text-fg">
        API key (optional)
      </label>
      <div className="flex gap-2">
        <input
          id={`generation-key-${kind}`}
          aria-label={`${kind} API key`}
          type="password"
          autoComplete="off"
          placeholder={saved ? "•••• saved in the keychain" : "Use the endpoint’s key"}
          value={key}
          onChange={(e) => setKey(e.target.value)}
          className={input}
        />
        <button
          type="button"
          className={button}
          disabled={key.trim() === "" || setSaved.isPending}
          onClick={() => setSaved.mutate(key.trim())}
        >
          Save key
        </button>
        {saved && (
          <button
            type="button"
            className={button}
            disabled={setSaved.isPending}
            onClick={() => setSaved.mutate(null)}
          >
            Remove
          </button>
        )}
      </div>
      <span className="text-sm text-fg-subtle">
        {setSaved.isError
          ? errorMessage(setSaved.error)
          : "Overrides the endpoint’s own key for this kind of generation only."}
      </span>
    </div>
  );
}
