import { Loader2, Play } from "lucide-react";
import { useEffect, useId, useRef, useState } from "react";
import {
  commands,
  type McpContent,
  type McpLogLine,
  type McpPrompt,
  type McpServerState,
  type McpTool,
  type McpToolResult,
} from "@/bindings";
import { errorMessage, unwrap } from "@/lib/api";
import { cn } from "@/lib/cn";
import { useMcpLists, useMcpLogs } from "./queries";
import { argumentSkeleton, parseJson } from "./schema";
import { useMcpUi } from "./store";
import { Button, inputClass, textareaClass } from "./ui";

type Tab = "tools" | "resources" | "prompts" | "logs";

/** A server's tools (with a runner), resources, prompts and log. */
export function ServerDetails({ server }: { server: McpServerState }) {
  const running = server.status === "running";
  const [tab, setTab] = useState<Tab>(running ? "tools" : "logs");
  const lists = useMcpLists(server.name, running);
  const tabs: Array<[Tab, string, number | null]> = [
    ["tools", "Tools", running ? lists.data.tools.length : null],
    [
      "resources",
      "Resources",
      running ? lists.data.resources.length + lists.data.templates.length : null,
    ],
    ["prompts", "Prompts", running ? lists.data.prompts.length : null],
    ["logs", "Log", null],
  ];
  return (
    <div>
      <div role="tablist" aria-label={`${server.name} details`} className="flex gap-1 px-4 pt-3">
        {tabs.map(([id, label, count]) => (
          <button
            key={id}
            role="tab"
            type="button"
            aria-selected={tab === id}
            onClick={() => setTab(id)}
            className={cn(
              "h-8 rounded-lg px-3 text-base transition-colors",
              tab === id ? "bg-active text-fg" : "text-fg-muted hover:bg-hover hover:text-fg",
            )}
          >
            {label}
            {count !== null && <span className="ml-1.5 text-fg-subtle">{count}</span>}
          </button>
        ))}
      </div>
      <div role="tabpanel" className="p-4">
        {tab === "logs" ? (
          <LogView server={server.name} />
        ) : !running ? (
          <p className="text-base text-fg-muted">The server isn’t running. Its log may say why.</p>
        ) : lists.isPending ? (
          <p className="text-fg-muted">Loading…</p>
        ) : tab === "tools" ? (
          <ToolList server={server.name} tools={lists.data.tools} />
        ) : tab === "resources" ? (
          <ResourceList
            server={server.name}
            resources={lists.data.resources}
            templates={lists.data.templates.map((t) => t.uriTemplate)}
          />
        ) : (
          <PromptList server={server.name} prompts={lists.data.prompts} />
        )}
        {server.instructions && tab === "tools" && running && (
          <details className="mt-4 text-sm text-fg-muted">
            <summary className="cursor-pointer">Server instructions</summary>
            <p className="mt-1 whitespace-pre-wrap">{server.instructions}</p>
          </details>
        )}
      </div>
    </div>
  );
}

function Empty({ children }: { children: string }) {
  return <p className="text-base text-fg-muted">{children}</p>;
}

function ToolList({ server, tools }: { server: string; tools: McpTool[] }) {
  const [open, setOpen] = useState<string | null>(null);
  if (tools.length === 0) return <Empty>This server has no tools.</Empty>;
  return (
    <ul className="flex flex-col gap-1">
      {tools.map((t) => {
        const a = t.annotations;
        const hints = [
          a.readOnlyHint && "read-only",
          a.destructiveHint && "destructive",
          a.openWorldHint && "open world",
        ].filter(Boolean);
        return (
          <li key={t.name} className="rounded-lg border border-line bg-app">
            <div className="flex items-start gap-3 px-3 py-2">
              <div className="min-w-0 flex-1">
                <div className="flex flex-wrap items-center gap-2">
                  <span className="font-mono text-sm text-fg">{t.name}</span>
                  {t.title && t.title !== t.name && (
                    <span className="text-sm text-fg-muted">{t.title}</span>
                  )}
                  {hints.map((h) => (
                    <span
                      key={String(h)}
                      className="rounded bg-sunken px-1.5 text-xs text-fg-subtle"
                    >
                      {h}
                    </span>
                  ))}
                </div>
                {t.description && (
                  <p className="mt-0.5 line-clamp-3 text-sm text-fg-muted">{t.description}</p>
                )}
              </div>
              <Button
                label={`Open the runner for ${t.name}`}
                onClick={() => setOpen((o) => (o === t.name ? null : t.name))}
              >
                <Play />
                Run
              </Button>
            </div>
            {open === t.name && <ToolRunner server={server} tool={t} />}
          </li>
        );
      })}
    </ul>
  );
}

let callSeq = 0;

/** Debug panel: edit the arguments as JSON, run the tool, see the result and progress. */
function ToolRunner({ server, tool }: { server: string; tool: McpTool }) {
  const id = useId();
  const [args, setArgs] = useState(() => argumentSkeleton(tool.inputSchema));
  const [token, setToken] = useState<string | null>(null);
  const [result, setResult] = useState<McpToolResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const progress = useMcpUi((s) => (token ? s.progress[token] : undefined));
  const clearProgress = useMcpUi((s) => s.clearProgress);

  const run = async () => {
    callSeq += 1;
    const t = `debug-${callSeq}`;
    setToken(t);
    setResult(null);
    setError(null);
    try {
      setResult(await unwrap(commands.mcpCallTool(server, tool.name, args, t)));
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setToken(null);
      clearProgress(t);
    }
  };

  return (
    <div className="border-t border-line px-3 py-3">
      <label htmlFor={`${id}-args`} className="mb-1 block text-sm font-medium text-fg-muted">
        Arguments (JSON)
      </label>
      <textarea
        id={`${id}-args`}
        value={args}
        onChange={(e) => setArgs(e.target.value)}
        rows={Math.min(12, Math.max(3, args.split("\n").length))}
        spellCheck={false}
        className={textareaClass}
      />
      <details className="mt-1 text-xs text-fg-subtle">
        <summary className="cursor-pointer">Input schema</summary>
        <pre className="mt-1 max-h-48 overflow-auto rounded bg-sunken p-2 font-mono">
          {JSON.stringify(parseJson(tool.inputSchema) ?? {}, null, 2)}
        </pre>
      </details>
      <div className="mt-2 flex items-center gap-3">
        <Button primary onClick={() => void run()} disabled={token !== null}>
          {token ? <Loader2 className="animate-spin" /> : <Play />}
          {token ? "Running…" : `Run ${tool.name}`}
        </Button>
        {progress && (
          <span role="status" className="text-sm text-fg-muted">
            {progress.message ?? "Working"}
            {progress.total ? ` (${progress.progress}/${progress.total})` : ""}
          </span>
        )}
      </div>
      {error && (
        <p role="alert" className="mt-3 rounded-lg bg-danger-soft px-3 py-2 text-base text-fg">
          {error}
        </p>
      )}
      {result && (
        <div className="mt-3" aria-label="Tool result">
          <p className={cn("mb-1 text-sm", result.isError ? "text-danger" : "text-fg-subtle")}>
            {result.isError ? "The tool reported an error" : "Result"} · {result.durationMs} ms
          </p>
          <ContentView content={result.content} />
          {result.structured && (
            <details className="mt-2 text-sm text-fg-muted">
              <summary className="cursor-pointer">Structured output</summary>
              <pre className="mt-1 max-h-60 overflow-auto rounded bg-sunken p-2 font-mono text-xs">
                {JSON.stringify(parseJson(result.structured) ?? result.structured, null, 2)}
              </pre>
            </details>
          )}
        </div>
      )}
    </div>
  );
}

/** Renders tool results, resources and prompt messages. */
export function ContentView({ content }: { content: McpContent[] }) {
  if (content.length === 0) return <p className="text-sm text-fg-subtle">(no content)</p>;
  return (
    <div className="flex flex-col gap-2">
      {content.map((c, i) => {
        switch (c.type) {
          case "text":
            return (
              <pre
                key={i}
                className="max-h-80 overflow-auto whitespace-pre-wrap rounded-lg bg-sunken p-3 font-mono text-xs text-fg"
              >
                {c.text}
              </pre>
            );
          case "image":
            return (
              <img
                key={i}
                alt="Tool output"
                src={`data:${c.mimeType};base64,${c.data}`}
                className="max-h-80 max-w-full rounded-lg border border-line"
              />
            );
          case "audio":
            return (
              <audio key={i} controls src={`data:${c.mimeType};base64,${c.data}`}>
                <track kind="captions" />
              </audio>
            );
          case "resourceLink":
            return (
              <p key={i} className="text-sm text-fg-muted">
                Link: <span className="font-mono">{c.name ?? c.uri}</span>{" "}
                <span className="text-fg-subtle">({c.uri})</span>
              </p>
            );
          case "resource":
            return c.text !== null ? (
              <div key={i}>
                <p className="mb-1 font-mono text-xs text-fg-subtle">{c.uri}</p>
                <pre className="max-h-80 overflow-auto whitespace-pre-wrap rounded-lg bg-sunken p-3 font-mono text-xs text-fg">
                  {c.text}
                </pre>
              </div>
            ) : c.blob && c.mimeType?.startsWith("image/") ? (
              <img
                key={i}
                alt={c.uri}
                src={`data:${c.mimeType};base64,${c.blob}`}
                className="max-h-80 max-w-full rounded-lg border border-line"
              />
            ) : (
              <p key={i} className="text-sm text-fg-muted">
                <span className="font-mono">{c.uri}</span> · binary {c.mimeType ?? "data"} (
                {Math.round(((c.blob?.length ?? 0) * 3) / 4 / 1024)} KB)
              </p>
            );
        }
      })}
    </div>
  );
}

function ResourceList({
  server,
  resources,
  templates,
}: {
  server: string;
  resources: Array<{ uri: string; name: string; title: string | null; mimeType: string | null }>;
  templates: string[];
}) {
  const [open, setOpen] = useState<string | null>(null);
  const [content, setContent] = useState<McpContent[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [custom, setCustom] = useState("");

  const read = async (uri: string) => {
    setOpen(uri);
    setContent(null);
    setError(null);
    try {
      setContent((await unwrap(commands.mcpReadResource(server, uri))).contents);
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  return (
    <div className="flex flex-col gap-3">
      {resources.length === 0 && templates.length === 0 && (
        <Empty>This server lists no resources.</Empty>
      )}
      {resources.length > 0 && (
        <ul className="flex flex-col gap-1">
          {resources.map((r) => (
            <li
              key={r.uri}
              className="flex items-center gap-3 rounded-lg border border-line bg-app px-3 py-2"
            >
              <div className="min-w-0 flex-1">
                <p className="text-sm text-fg">{r.title ?? r.name}</p>
                <p className="truncate font-mono text-xs text-fg-subtle">{r.uri}</p>
              </div>
              <Button label={`Read ${r.name}`} onClick={() => void read(r.uri)}>
                Read
              </Button>
            </li>
          ))}
        </ul>
      )}
      {templates.length > 0 && (
        <div className="text-sm text-fg-muted">
          <p className="mb-1">
            Templates:{" "}
            {templates.map((t) => (
              <code key={t} className="mr-2 font-mono text-xs">
                {t}
              </code>
            ))}
          </p>
          <form
            className="flex gap-2"
            onSubmit={(e) => {
              e.preventDefault();
              if (custom.trim()) void read(custom.trim());
            }}
          >
            <input
              aria-label="Resource URI"
              value={custom}
              onChange={(e) => setCustom(e.target.value)}
              placeholder="Fill in a template's URI"
              className={cn(inputClass, "h-9 font-mono text-sm")}
            />
            <Button type="submit">Read</Button>
          </form>
        </div>
      )}
      {open && (
        <div aria-label="Resource contents">
          {error ? (
            <p role="alert" className="text-danger">
              {error}
            </p>
          ) : content ? (
            <ContentView content={content} />
          ) : (
            <p className="text-fg-muted">Reading…</p>
          )}
        </div>
      )}
    </div>
  );
}

function PromptList({ server, prompts }: { server: string; prompts: McpPrompt[] }) {
  const [open, setOpen] = useState<string | null>(null);
  if (prompts.length === 0) return <Empty>This server has no prompts.</Empty>;
  return (
    <ul className="flex flex-col gap-1">
      {prompts.map((p) => (
        <li key={p.name} className="rounded-lg border border-line bg-app">
          <div className="flex items-start gap-3 px-3 py-2">
            <div className="min-w-0 flex-1">
              <p className="font-mono text-sm text-fg">{p.name}</p>
              {p.description && <p className="mt-0.5 text-sm text-fg-muted">{p.description}</p>}
            </div>
            <Button
              label={`Try ${p.name}`}
              onClick={() => setOpen((o) => (o === p.name ? null : p.name))}
            >
              Try
            </Button>
          </div>
          {open === p.name && <PromptRunner server={server} prompt={p} />}
        </li>
      ))}
    </ul>
  );
}

function PromptRunner({ server, prompt }: { server: string; prompt: McpPrompt }) {
  const id = useId();
  const [values, setValues] = useState<Record<string, string>>({});
  const [content, setContent] = useState<McpContent[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const get = async () => {
    setError(null);
    try {
      const r = await unwrap(commands.mcpGetPrompt(server, prompt.name, values));
      setContent(r.messages.map((m) => m.content));
    } catch (e) {
      setError(errorMessage(e));
    }
  };
  return (
    <div className="flex flex-col gap-2 border-t border-line px-3 py-3">
      {prompt.arguments.map((a) => (
        <div key={a.name}>
          <label htmlFor={`${id}-${a.name}`} className="mb-1 block text-sm text-fg-muted">
            {a.title ?? a.name}
            {a.required && " *"}
          </label>
          <input
            id={`${id}-${a.name}`}
            value={values[a.name] ?? ""}
            placeholder={a.description ?? ""}
            onChange={(e) => setValues((v) => ({ ...v, [a.name]: e.target.value }))}
            className={cn(inputClass, "h-9")}
          />
        </div>
      ))}
      <div>
        <Button primary onClick={() => void get()}>
          Get prompt
        </Button>
      </div>
      {error && (
        <p role="alert" className="text-danger">
          {error}
        </p>
      )}
      {content && <ContentView content={content} />}
    </div>
  );
}

function LogView({ server }: { server: string }) {
  const { data: lines, isPending } = useMcpLogs(server);
  return <LogLines lines={lines} loading={isPending} />;
}

/** A scrolling log that follows new lines while scrolled to the bottom. */
export function LogLines({ lines, loading }: { lines: McpLogLine[]; loading?: boolean }) {
  const ref = useRef<HTMLDivElement>(null);
  const stick = useRef(true);
  useEffect(() => {
    const el = ref.current;
    if (el && stick.current) el.scrollTop = el.scrollHeight;
  }, [lines]);
  return (
    <div
      ref={ref}
      role="log"
      aria-label="Server log"
      onScroll={(e) => {
        const el = e.currentTarget;
        stick.current = el.scrollHeight - el.scrollTop - el.clientHeight < 24;
      }}
      className="h-64 overflow-auto rounded-lg bg-sunken p-3 font-mono text-xs leading-5"
    >
      {loading ? (
        <p className="text-fg-subtle">Loading…</p>
      ) : lines.length === 0 ? (
        <p className="text-fg-subtle">Nothing logged yet.</p>
      ) : (
        lines.map((l, i) => (
          <div
            key={i}
            className={cn(
              "whitespace-pre-wrap break-words",
              l.source === "oxide" && "text-accent",
              l.source === "stderr" && "text-fg-muted",
              l.source === "server" && "text-fg",
              (l.level === "error" || l.level === "critical") && "text-danger",
            )}
          >
            <span className="mr-2 text-fg-subtle">{new Date(l.at).toLocaleTimeString()}</span>
            {l.level && l.source === "server" && <span className="mr-1">[{l.level}]</span>}
            {l.text}
          </div>
        ))
      )}
    </div>
  );
}
