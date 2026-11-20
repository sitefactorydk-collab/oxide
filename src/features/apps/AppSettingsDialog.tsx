import { Dialog } from "radix-ui";
import { X } from "lucide-react";
import { useEffect, useState } from "react";
import type {
  AppCapability,
  AppGenerationOverride,
  AppPrefs,
  GenerationKind,
  UtilityApp,
} from "@/bindings";
import { IconButton } from "@/components/ui/IconButton";
import { useModels } from "@/features/models/queries";
import { useProviders } from "@/features/providers/queries";
import { errorMessage } from "@/lib/api";
import { formatBytes } from "@/lib/time";
import { toast } from "@/lib/toast";
import {
  CAPABILITY_TEXT,
  useAppPrefs,
  useClearAppStorage,
  useSaveAppPrefs,
  useSetAppKey,
} from "./queries";

const KINDS: Array<{ kind: GenerationKind; cap: AppCapability; label: string }> = [
  { kind: "image", cap: "image", label: "Image generation" },
  { kind: "video", cap: "video", label: "Video generation" },
  { kind: "music", cap: "music", label: "Music generation" },
];

const field =
  "h-9 w-full rounded-lg border border-line bg-raised px-3 text-base text-fg outline-none focus:border-accent";

/** An app's settings: its models, its own keys, what it may do, its storage. */
export function AppSettingsDialog({
  app,
  open,
  onOpenChange,
}: {
  app: UtilityApp;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-40 bg-overlay" />
        <Dialog.Content
          aria-describedby={undefined}
          className="ox-pop fixed left-1/2 top-1/2 z-50 flex max-h-[min(720px,calc(100vh-48px))] w-[min(600px,calc(100vw-48px))] -translate-x-1/2 -translate-y-1/2 flex-col rounded-2xl border border-line bg-app shadow-lg outline-none"
        >
          <div className="flex items-center justify-between border-b border-line px-6 py-4">
            <Dialog.Title className="font-serif text-xl font-normal text-fg">
              {app.manifest.name} settings
            </Dialog.Title>
            <Dialog.Close asChild>
              <IconButton tooltip={false} size="sm" label="Close">
                <X className="size-4" />
              </IconButton>
            </Dialog.Close>
          </div>
          {open && <SettingsBody app={app} onDone={() => onOpenChange(false)} />}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

function SettingsBody({ app, onDone }: { app: UtilityApp; onDone: () => void }) {
  const id = app.manifest.id;
  const { data: view } = useAppPrefs(id);
  const { data: models } = useModels();
  const save = useSaveAppPrefs(id);
  const clear = useClearAppStorage(id);
  const [prefs, setPrefs] = useState<AppPrefs | null>(null);
  useEffect(() => {
    if (view && !prefs) setPrefs(view.prefs);
  }, [view, prefs]);
  if (!view || !prefs) return <p className="p-6 text-fg-muted">Loading…</p>;

  const declared = app.manifest.capabilities ?? [];
  const kinds = KINDS.filter((k) => declared.includes(k.cap));
  const setOverride = (kind: GenerationKind, o: AppGenerationOverride) =>
    setPrefs({ ...prefs, [kind]: o });
  const revoke = (cap: AppCapability) => {
    const next = { ...prefs, granted: prefs.granted.filter((c) => c !== cap) };
    setPrefs(next);
    save.mutate(next, { onError: (e) => toast(errorMessage(e), "error") });
  };

  return (
    <>
      <div className="flex min-h-0 flex-1 flex-col gap-6 overflow-y-auto px-6 py-5 text-base">
        {declared.includes("text") && (
          <section className="flex flex-col gap-1.5">
            <label htmlFor="app-text-model" className="font-medium text-fg">
              Text model
            </label>
            <select
              id="app-text-model"
              className={field}
              value={prefs.textModel ?? ""}
              onChange={(e) => setPrefs({ ...prefs, textModel: e.target.value || null })}
            >
              <option value="">
                Default{view.textModel && !prefs.textModel ? ` (${view.textModel})` : ""}
              </option>
              {models.map((p) => (
                <optgroup key={p.providerId} label={p.providerName}>
                  {p.models.map((m) => (
                    <option key={m.id} value={m.id}>
                      {m.label}
                    </option>
                  ))}
                </optgroup>
              ))}
            </select>
            <p className="text-sm text-fg-subtle">
              A chat model from Settings → Providers and Models. Default: the one the app names,
              else the first available.
            </p>
          </section>
        )}

        {kinds.map((k) => (
          <GenerationOverride
            key={k.kind}
            appId={id}
            label={k.label}
            kind={k.kind}
            value={prefs[k.kind]}
            hasKey={view.keys.includes(k.kind)}
            manifestModel={app.manifest.models?.[k.kind] ?? null}
            onChange={(o) => setOverride(k.kind, o)}
          />
        ))}

        <section className="flex flex-col gap-2">
          <h3 className="font-medium text-fg">Permissions</h3>
          {declared.length === 0 ? (
            <p className="text-fg-muted">This app uses no models or tools.</p>
          ) : (
            <ul className="flex flex-col divide-y divide-line rounded-lg border border-line">
              {declared.map((c) => {
                const granted = prefs.granted.includes(c);
                return (
                  <li key={c} className="flex items-center gap-3 px-3 py-2">
                    <span className="flex-1 text-fg">{CAPABILITY_TEXT[c].label}</span>
                    {granted ? (
                      <>
                        <span className="text-sm text-success">Allowed</span>
                        <button
                          type="button"
                          onClick={() => revoke(c)}
                          className="h-7 rounded-md border border-line px-2 text-sm text-fg hover:bg-hover"
                        >
                          Revoke
                        </button>
                      </>
                    ) : (
                      <span className="text-sm text-fg-subtle">Asks when used</span>
                    )}
                  </li>
                );
              })}
            </ul>
          )}
        </section>

        <section className="flex items-center gap-3">
          <div className="flex-1">
            <h3 className="font-medium text-fg">Storage</h3>
            <p className="text-sm text-fg-muted">
              {formatBytes(view.storageBytes)} saved by the app.
            </p>
          </div>
          <button
            type="button"
            disabled={view.storageBytes === 0 || clear.isPending}
            onClick={() =>
              clear.mutate(undefined, {
                onSuccess: () => toast("The app's storage is cleared."),
                onError: (e) => toast(errorMessage(e), "error"),
              })
            }
            className="h-8 rounded-lg border border-line px-3 text-sm text-fg hover:bg-hover disabled:opacity-40"
          >
            Clear storage
          </button>
        </section>
      </div>
      <div className="flex justify-end gap-2 border-t border-line px-6 py-4">
        <button
          type="button"
          onClick={onDone}
          className="h-9 rounded-lg border border-line px-3.5 text-base text-fg hover:bg-hover"
        >
          Cancel
        </button>
        <button
          type="button"
          disabled={save.isPending}
          onClick={() =>
            save.mutate(prefs, {
              onSuccess: onDone,
              onError: (e) => toast(errorMessage(e), "error"),
            })
          }
          className="h-9 rounded-lg bg-fg px-3.5 text-base font-medium text-app hover:opacity-90 disabled:opacity-40"
        >
          Save
        </button>
      </div>
    </>
  );
}

function GenerationOverride({
  appId,
  label,
  kind,
  value,
  hasKey,
  manifestModel,
  onChange,
}: {
  appId: string;
  label: string;
  kind: GenerationKind;
  value: AppGenerationOverride;
  hasKey: boolean;
  manifestModel: string | null;
  onChange: (o: AppGenerationOverride) => void;
}) {
  const { data: providers } = useProviders();
  const setKey = useSetAppKey(appId);
  const [key, setKeyText] = useState("");
  const endpoints = (providers ?? []).filter((p) => p.kind !== "anthropic");
  return (
    <section className="flex flex-col gap-2">
      <h3 className="font-medium text-fg">{label}</h3>
      <p className="text-sm text-fg-subtle">
        Empty fields use the default in Settings → Media generation
        {manifestModel ? ` (the app suggests ${manifestModel})` : ""}.
      </p>
      <div className="grid grid-cols-2 gap-2">
        <label className="flex flex-col gap-1 text-sm text-fg-muted">
          Endpoint
          <select
            className={field}
            value={value.providerId ?? ""}
            onChange={(e) => onChange({ ...value, providerId: e.target.value || null })}
          >
            <option value="">Default</option>
            {endpoints.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </select>
        </label>
        <label className="flex flex-col gap-1 text-sm text-fg-muted">
          Model
          <input
            className={field}
            value={value.model}
            placeholder={manifestModel ?? "Default"}
            onChange={(e) => onChange({ ...value, model: e.target.value })}
          />
        </label>
        <label className="col-span-2 flex flex-col gap-1 text-sm text-fg-muted">
          Custom URL (wins over the endpoint)
          <input
            className={field}
            value={value.baseUrl}
            placeholder="https://…/v1"
            onChange={(e) => onChange({ ...value, baseUrl: e.target.value })}
          />
        </label>
        <label className="col-span-2 flex flex-col gap-1 text-sm text-fg-muted">
          API key of this app {hasKey && <span className="text-success">(saved)</span>}
          <span className="flex gap-2">
            <input
              type="password"
              className={field}
              value={key}
              autoComplete="off"
              placeholder={hasKey ? "••••••••" : "Optional: overrides the endpoint's key"}
              onChange={(e) => setKeyText(e.target.value)}
            />
            <button
              type="button"
              disabled={!key.trim() || setKey.isPending}
              onClick={() =>
                setKey.mutate(
                  { kind, key: key.trim() },
                  {
                    onSuccess: () => setKeyText(""),
                    onError: (e) => toast(errorMessage(e), "error"),
                  },
                )
              }
              className="h-9 shrink-0 rounded-lg border border-line px-3 text-sm text-fg hover:bg-hover disabled:opacity-40"
            >
              Save key
            </button>
            {hasKey && (
              <button
                type="button"
                onClick={() =>
                  setKey.mutate(
                    { kind, key: null },
                    { onError: (e) => toast(errorMessage(e), "error") },
                  )
                }
                className="h-9 shrink-0 rounded-lg border border-line px-3 text-sm text-fg hover:bg-hover"
              >
                Remove
              </button>
            )}
          </span>
        </label>
      </div>
    </section>
  );
}
