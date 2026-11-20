import { useState } from "react";
import {
  AlertCircle,
  Check,
  Code,
  Copy,
  FileCode,
  Link as LinkIcon,
  Loader2,
  MessageSquare,
  Search,
  Sparkles,
  Trash2,
  X,
} from "lucide-react";
import { ai, app, web } from "oxide";

interface Suggestion {
  title: string;
  description: string;
  reasoning: string;
}

const SAMPLE_HTML = `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <title>CloudFlow Analytics - Real-Time Enterprise Data Platform</title>
  <meta name="description" content="Analyze big data pipelines with sub-second latency.">
</head>
<body>
  <header><h1>Next-Generation Data Intelligence for Modern Engineering Teams</h1></header>
  <main>
    <p>CloudFlow provides real-time streaming analytics, automated anomaly detection, and unified observability for high-throughput cloud architectures.</p>
    <h2>Key Capabilities</h2>
    <ul>
      <li>Zero-copy parquet ingestion</li>
      <li>Millisecond query latencies across petabyte datasets</li>
      <li>Native SOC-2 Type II and HIPAA compliance encryption</li>
    </ul>
    <p>Get started with a 14-day free trial. No credit card required.</p>
  </main>
</body>
</html>`;

const SCHEMA = {
  type: "object",
  properties: {
    suggestions: {
      type: "array",
      items: {
        type: "object",
        properties: {
          title: { type: "string" },
          description: { type: "string" },
          reasoning: { type: "string" },
        },
        required: ["title", "description", "reasoning"],
      },
    },
  },
  required: ["suggestions"],
};

function prompt(content: string, source: string, comments: string): string {
  return `I need you to act as an SEO expert.

Task: analyze the content, existing meta tags, headings and core purpose of the following web page (${source}).

The raw HTML or text of the page is below. Use it to understand its core topics, target audience, primary keywords, brand name and intent.

--- START OF WEBSITE CONTENT ---
${content.slice(0, 100_000)}
--- END OF WEBSITE CONTENT ---
${comments ? `\nAdditional guidelines from the user:\n${comments}\n` : ""}
Based on this content, write 3 distinct, high-quality, search-optimized pairs of meta titles and meta descriptions.

Requirements:
1. Title length: 50-60 characters.
2. Description length: 150-160 characters.
3. Include the primary keywords naturally.
4. Make them compelling to click (CTR optimization).

Return {"suggestions": [{"title": "...", "description": "...", "reasoning": "a brief explanation"}]}.`;
}

function lengthClass(n: number, min: number, max: number): string {
  return n < min || n > max
    ? "text-amber-600 dark:text-amber-400"
    : "text-emerald-600 dark:text-emerald-400";
}

export default function App() {
  const [mode, setMode] = useState<"url" | "html">("url");
  const [url, setUrl] = useState("");
  const [html, setHtml] = useState("");
  const [context, setContext] = useState("");
  const [comments, setComments] = useState("");
  const [busy, setBusy] = useState<"" | "fetching" | "writing">("");
  const [error, setError] = useState<string | null>(null);
  const [results, setResults] = useState<Suggestion[] | null>(null);
  const [source, setSource] = useState("");
  const [model, setModel] = useState("");
  const [showSource, setShowSource] = useState(false);
  const [copied, setCopied] = useState<string | null>(null);

  const copy = async (text: string, id: string) => {
    await app.copy(text);
    setCopied(id);
    setTimeout(() => setCopied(null), 2000);
  };

  const generate = async () => {
    setError(null);
    let content = html.trim();
    let label = `pasted HTML source (${context.trim() || "no name given"})`;
    if (mode === "url") {
      try {
        new URL(url.trim());
      } catch {
        setError("Enter a valid URL, like https://example.com/page");
        return;
      }
      label = `URL: ${url.trim()}`;
    } else if (content.length < 15) {
      setError("Paste the page's HTML source (at least 15 characters).");
      return;
    }
    setResults(null);
    try {
      if (mode === "url") {
        setBusy("fetching");
        const page = await web.fetch(url.trim(), { format: "html" });
        content = page.content;
      }
      setBusy("writing");
      const answer = await ai.json<{ suggestions?: Suggestion[] }>({
        prompt: prompt(content, label, comments.trim()),
        schema: SCHEMA,
      });
      const list = (answer.suggestions ?? []).filter((s) => s && s.title && s.description);
      if (list.length === 0) throw new Error("The model returned no suggestions. Try again.");
      setResults(list.slice(0, 3));
      setSource(content);
      const info = await ai.info();
      setModel(info.textModel ?? "");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy("");
    }
  };

  const disabled = busy !== "" || (mode === "url" ? !url.trim() : !html.trim());
  const tab = (active: boolean) =>
    `flex items-center justify-center gap-2 py-2 px-3 rounded-lg text-sm font-medium transition-all ${
      active
        ? "bg-white dark:bg-neutral-800 text-neutral-900 dark:text-white shadow-sm"
        : "text-neutral-600 dark:text-neutral-400 hover:text-neutral-900 dark:hover:text-white"
    }`;
  const input =
    "w-full rounded-xl border border-neutral-200 dark:border-neutral-800 bg-white dark:bg-neutral-900 text-neutral-900 dark:text-white placeholder:text-neutral-400 focus:ring-2 focus:ring-neutral-900 dark:focus:ring-white outline-none text-sm";

  return (
    <div className="mx-auto max-w-6xl p-6">
      <header className="mb-8 text-center">
        <h1 className="mb-3 flex items-center justify-center gap-3 text-3xl font-bold text-neutral-900 dark:text-white">
          <span className="rounded-xl bg-neutral-900 p-2 text-white dark:bg-white dark:text-neutral-900">
            <Search className="h-7 w-7" />
          </span>
          AI Meta Tag Generator
        </h1>
        <p className="mx-auto max-w-2xl text-neutral-600 dark:text-neutral-400">
          Optimized meta titles and descriptions from a live URL or the page's source HTML.
        </p>
      </header>

      <section className="mb-8 space-y-6 rounded-2xl border border-neutral-200 bg-white p-6 shadow-sm dark:border-neutral-800 dark:bg-neutral-950">
        <div className="grid max-w-md grid-cols-2 gap-2 rounded-xl bg-neutral-100 p-1 dark:bg-neutral-900">
          <button type="button" className={tab(mode === "url")} onClick={() => setMode("url")}>
            <LinkIcon className="h-4 w-4" /> Web page URL
          </button>
          <button type="button" className={tab(mode === "html")} onClick={() => setMode("html")}>
            <FileCode className="h-4 w-4" /> Paste HTML source
          </button>
        </div>

        {mode === "url" ? (
          <label className="block space-y-2">
            <span className="text-sm font-medium text-neutral-700 dark:text-neutral-300">
              Web page URL
            </span>
            <input
              type="url"
              className={`${input} px-4 py-3`}
              placeholder="https://example.com/page"
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && !disabled && void generate()}
              disabled={busy !== ""}
            />
          </label>
        ) : (
          <div className="space-y-3">
            <div className="flex items-center justify-between">
              <span className="text-sm font-medium text-neutral-700 dark:text-neutral-300">
                Page HTML source
              </span>
              <span className="flex gap-3 text-xs">
                <button
                  type="button"
                  className="inline-flex items-center gap-1 font-medium text-blue-600 hover:underline dark:text-blue-400"
                  onClick={() => {
                    setHtml(SAMPLE_HTML);
                    setContext("CloudFlow Enterprise Data Platform");
                  }}
                >
                  <Sparkles className="h-3.5 w-3.5" /> Sample HTML
                </button>
                {html && (
                  <button
                    type="button"
                    className="inline-flex items-center gap-1 text-neutral-500 hover:text-red-600"
                    onClick={() => setHtml("")}
                  >
                    <Trash2 className="h-3.5 w-3.5" /> Clear
                  </button>
                )}
              </span>
            </div>
            <textarea
              rows={9}
              className={`${input} bg-neutral-50 p-4 font-mono text-xs dark:bg-neutral-900`}
              placeholder="<!DOCTYPE html>…"
              value={html}
              onChange={(e) => setHtml(e.target.value)}
              disabled={busy !== ""}
            />
            <div className="text-right text-xs text-neutral-500">
              {html.length.toLocaleString()} characters
            </div>
            <input
              type="text"
              className={`${input} px-4 py-2.5`}
              placeholder="Target URL or page name (optional context)"
              value={context}
              onChange={(e) => setContext(e.target.value)}
              disabled={busy !== ""}
            />
          </div>
        )}

        <label className="block space-y-2">
          <span className="flex items-center gap-2 text-sm font-medium text-neutral-700 dark:text-neutral-300">
            <MessageSquare className="h-4 w-4" /> Optional comments / SEO guidelines
          </span>
          <textarea
            className={`${input} h-20 px-4 py-3`}
            placeholder="E.g. target keyword 'enterprise data platform', brand 'CloudFlow', highlight the free trial…"
            value={comments}
            onChange={(e) => setComments(e.target.value)}
            disabled={busy !== ""}
          />
        </label>

        <button
          type="button"
          onClick={() => void generate()}
          disabled={disabled}
          className="flex h-12 w-full items-center justify-center gap-2 rounded-xl bg-neutral-900 text-sm font-semibold text-white hover:bg-neutral-800 disabled:cursor-not-allowed disabled:bg-neutral-300 dark:bg-white dark:text-neutral-900 dark:disabled:bg-neutral-800"
        >
          {busy ? (
            <>
              <Loader2 className="h-5 w-5 animate-spin" />
              {busy === "fetching" ? "Reading the page…" : "Writing meta tags…"}
            </>
          ) : (
            <>
              <Search className="h-4 w-4" /> Generate meta tags
            </>
          )}
        </button>

        {error && (
          <div className="flex items-start gap-3 rounded-xl border border-red-200 bg-red-50 p-4 text-sm font-medium text-red-800 dark:border-red-900/60 dark:bg-red-950/40 dark:text-red-300">
            <AlertCircle className="mt-0.5 h-5 w-5 shrink-0" />
            {error}
          </div>
        )}
      </section>

      {results && (
        <section className="rounded-2xl border border-neutral-200 bg-white p-6 shadow-sm dark:border-neutral-800 dark:bg-neutral-950">
          <div className="mb-6 flex flex-wrap items-center justify-between gap-3 border-b border-neutral-200 pb-4 dark:border-neutral-800">
            <div className="flex items-center gap-3">
              <h2 className="text-xl font-bold text-neutral-900 dark:text-white">Suggestions</h2>
              <button
                type="button"
                onClick={() => setShowSource(true)}
                className="flex items-center gap-1.5 rounded-lg border border-neutral-200 px-3 py-1.5 text-xs font-medium text-neutral-700 hover:bg-neutral-100 dark:border-neutral-800 dark:text-neutral-300 dark:hover:bg-neutral-800"
              >
                <Code className="h-3.5 w-3.5" /> View source
              </button>
            </div>
            {model && (
              <span className="rounded-md bg-neutral-100 px-3 py-1 font-mono text-xs text-neutral-500 dark:bg-neutral-800 dark:text-neutral-400">
                {model}
              </span>
            )}
          </div>
          <div className="grid grid-cols-1 gap-6 md:grid-cols-3">
            {results.map((s, i) => (
              <article
                key={i}
                className="flex flex-col rounded-xl border border-neutral-200 bg-neutral-50 p-5 dark:border-neutral-800 dark:bg-neutral-900/60"
              >
                <span className="mb-4 w-fit rounded-full bg-neutral-200 px-3 py-1 text-xs font-bold uppercase tracking-wider text-neutral-700 dark:bg-neutral-800 dark:text-neutral-300">
                  Option {i + 1}
                </span>
                {(
                  [
                    [
                      "Meta title",
                      s.title,
                      50,
                      60,
                      "font-semibold text-neutral-900 dark:text-white",
                    ],
                    [
                      "Meta description",
                      s.description,
                      150,
                      160,
                      "text-sm text-neutral-700 dark:text-neutral-300",
                    ],
                  ] as const
                ).map(([label, text, min, max, cls]) => (
                  <div key={label} className="mb-4">
                    <div className="mb-2 flex items-end justify-between">
                      <span className="text-xs font-bold uppercase tracking-wider text-neutral-500">
                        {label}
                      </span>
                      <span
                        className={`font-mono text-xs font-semibold ${lengthClass(text.length, min, max)}`}
                      >
                        {text.length} chars
                      </span>
                    </div>
                    <div className="relative rounded-lg border border-neutral-200 bg-white p-4 pr-12 dark:border-neutral-800 dark:bg-neutral-900">
                      <p className={cls}>{text}</p>
                      <button
                        type="button"
                        title={`Copy ${label.toLowerCase()}`}
                        onClick={() => void copy(text, `${label}-${i}`)}
                        className="absolute right-3 top-3 rounded-md p-1.5 text-neutral-500 hover:bg-neutral-100 dark:hover:bg-neutral-800"
                      >
                        {copied === `${label}-${i}` ? (
                          <Check className="h-4 w-4 text-emerald-600" />
                        ) : (
                          <Copy className="h-4 w-4" />
                        )}
                      </button>
                    </div>
                  </div>
                ))}
                <p className="mt-auto border-t border-neutral-200 pt-4 text-xs text-neutral-600 dark:border-neutral-800 dark:text-neutral-400">
                  <strong className="text-neutral-900 dark:text-white">Why this works:</strong>{" "}
                  <em>{s.reasoning}</em>
                </p>
              </article>
            ))}
          </div>
        </section>
      )}

      {showSource && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-6">
          <div className="flex max-h-[85vh] w-full max-w-4xl flex-col overflow-hidden rounded-2xl border border-slate-700 bg-slate-800 shadow-2xl">
            <div className="flex items-center justify-between border-b border-slate-700 px-6 py-4">
              <h3 className="flex items-center gap-2 font-bold text-white">
                <Code className="h-5 w-5 text-blue-400" /> Analyzed page source
              </h3>
              <div className="flex items-center gap-2">
                <button
                  type="button"
                  onClick={() => void copy(source, "source")}
                  className="inline-flex items-center gap-1.5 rounded-lg bg-slate-700 px-3 py-1 text-xs font-medium text-slate-200 hover:bg-slate-600"
                >
                  {copied === "source" ? (
                    <Check className="h-3.5 w-3.5" />
                  ) : (
                    <Copy className="h-3.5 w-3.5" />
                  )}
                  {copied === "source" ? "Copied" : "Copy"}
                </button>
                <button
                  type="button"
                  aria-label="Close"
                  onClick={() => setShowSource(false)}
                  className="p-1 text-slate-400 hover:text-white"
                >
                  <X className="h-5 w-5" />
                </button>
              </div>
            </div>
            <pre className="flex-1 overflow-auto whitespace-pre-wrap bg-slate-900/70 p-6 font-mono text-xs leading-relaxed text-slate-300">
              {source}
            </pre>
          </div>
        </div>
      )}
    </div>
  );
}
