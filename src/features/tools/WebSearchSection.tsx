import { Check, Loader2 } from "lucide-react";
import { useEffect, useState } from "react";
import type { SearchKeyKind, WebSearchProvider } from "@/bindings";
import { Toggle } from "@/features/models/ModelsSection";
import {
  DEFAULT_SETTINGS,
  useSetWebSearchSettings,
  useSettings,
} from "@/features/settings/queries";
import { errorMessage } from "@/lib/api";
import { cn } from "@/lib/cn";
import { useSearchKeys, useSetSearchKey, useTestSearch } from "./queries";

const PROVIDERS: Array<{ id: WebSearchProvider; label: string; hint: string }> = [
  { id: "none", label: "Off", hint: "Models get no web search tool (web fetch still works)." },
  { id: "brave", label: "Brave Search", hint: "Brave Search API. Needs an API key." },
  { id: "tavily", label: "Tavily", hint: "Search made for AI agents. Needs an API key." },
  { id: "searxng", label: "SearXNG", hint: "Your own SearXNG instance, with JSON output on." },
];

/** Settings → Web search: which service `web_search` uses, keys and a test. */
export function WebSearchSection() {
  const { data } = useSettings();
  const ws = (data ?? DEFAULT_SETTINGS).webSearch;
  const set = useSetWebSearchSettings();
  const keys = useSearchKeys();
  const setKey = useSetSearchKey();
  const test = useTestSearch();
  const [typed, setTyped] = useState("");
  const [url, setUrl] = useState(ws.searxngUrl);
  useEffect(() => setUrl(ws.searxngUrl), [ws.searxngUrl]);
  useEffect(() => {
    setTyped("");
    test.reset();
    // Reset when the service changes only.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ws.provider]);

  const keyKind: SearchKeyKind | null =
    ws.provider === "brave" ? "brave" : ws.provider === "tavily" ? "tavily" : null;
  const hint = keyKind ? keys.data?.[keyKind] : null;

  return (
    <div className="flex max-w-2xl flex-col gap-6">
      <fieldset className="flex flex-col gap-2">
        <legend className="mb-2 text-md font-medium text-fg">Search service</legend>
        {PROVIDERS.map((p) => (
          <label
            key={p.id}
            className={cn(
              "flex cursor-pointer items-start gap-3 rounded-xl border px-3 py-2.5",
              ws.provider === p.id ? "border-accent bg-accent-soft" : "border-line bg-raised",
            )}
          >
            <input
              type="radio"
              name="search-provider"
              checked={ws.provider === p.id}
              onChange={() => set({ provider: p.id })}
              className="mt-1 accent-[var(--ox-accent)]"
            />
            <span>
              <span className="block text-md text-fg">{p.label}</span>
              <span className="block text-sm text-fg-subtle">{p.hint}</span>
            </span>
          </label>
        ))}
      </fieldset>

      {keyKind && (
        <div className="flex flex-col gap-2">
          <span className="text-md font-medium text-fg">API key</span>
          <div className="flex gap-2">
            <input
              type="password"
              aria-label="API key"
              autoComplete="off"
              placeholder={hint ? `Saved (${hint})` : "Paste your key"}
              value={typed}
              onChange={(e) => setTyped(e.target.value)}
              className="h-10 min-w-0 flex-1 rounded-lg border border-line bg-raised px-3 text-md text-fg outline-none focus:border-accent"
            />
            <button
              type="button"
              disabled={typed.trim() === "" || setKey.isPending}
              onClick={() =>
                setKey.mutate(
                  { service: keyKind, key: typed.trim() },
                  { onSuccess: () => setTyped("") },
                )
              }
              className="h-10 rounded-lg bg-fg px-4 text-sm font-medium text-app disabled:opacity-40"
            >
              Save key
            </button>
            {hint && (
              <button
                type="button"
                onClick={() => setKey.mutate({ service: keyKind, key: null })}
                className="h-10 rounded-lg px-3 text-sm text-fg-muted hover:bg-hover hover:text-fg"
              >
                Remove
              </button>
            )}
          </div>
          {setKey.error && <p className="text-sm text-danger">{errorMessage(setKey.error)}</p>}
        </div>
      )}

      {ws.provider === "searxng" && (
        <label className="flex flex-col gap-2">
          <span className="text-md font-medium text-fg">SearXNG URL</span>
          <input
            type="url"
            placeholder="http://localhost:8888"
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            onBlur={() => url !== ws.searxngUrl && set({ searxngUrl: url.trim() })}
            className="h-10 rounded-lg border border-line bg-raised px-3 text-md text-fg outline-none focus:border-accent"
          />
          <span className="text-sm text-fg-subtle">
            Oxide calls <code>/search?format=json</code>. Enable <code>json</code> under{" "}
            <code>search.formats</code> in the instance’s settings.yml.
          </span>
        </label>
      )}

      {ws.provider !== "none" && (
        <div className="flex items-center gap-3">
          <button
            type="button"
            disabled={test.isPending}
            onClick={() =>
              test.mutate({
                settings: { ...ws, searxngUrl: url.trim() },
                key: typed.trim() || null,
              })
            }
            className="inline-flex h-9 items-center gap-2 rounded-lg border border-line px-3 text-sm text-fg-muted hover:bg-hover hover:text-fg disabled:opacity-60"
          >
            {test.isPending && <Loader2 className="size-4 animate-spin" />}
            Test search
          </button>
          {test.data && (
            <p role="status" className={cn("text-sm", test.data.ok ? "text-fg" : "text-danger")}>
              {test.data.ok && <Check className="mr-1 inline size-4 text-success" />}
              {test.data.message}
              {test.data.sample.length > 0 && (
                <span className="text-fg-subtle"> {test.data.sample.join(" · ")}</span>
              )}
            </p>
          )}
          {test.error && <p className="text-sm text-danger">{errorMessage(test.error)}</p>}
        </div>
      )}

      <div className="border-t border-line pt-6">
        <Toggle
          label="Use Anthropic’s web search on Anthropic models"
          hint="Anthropic runs the search itself (billed by Anthropic per search). When off, Anthropic models use the service above like the others."
          checked={ws.anthropicNative}
          onChange={(anthropicNative) => set({ anthropicNative })}
        />
      </div>
      <p className="text-sm text-fg-subtle">
        Web search can be switched off per chat in the composer’s “+” menu. Web fetch (reading a
        page by URL) needs no service.
      </p>
    </div>
  );
}
