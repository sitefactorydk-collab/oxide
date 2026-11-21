import { useRef, useState } from "react";
import {
  AlertCircle,
  Check,
  CheckCircle,
  Copy,
  Download,
  ExternalLink,
  FileText,
  Globe,
  HelpCircle,
  Loader2,
  Square,
} from "lucide-react";
import { ai, app, files, web } from "oxide";

export interface PageRef {
  url: string;
  title: string;
}

export interface LanguageGroup {
  language: string;
  pages: PageRef[];
}

export interface ScanResult {
  highValue: LanguageGroup[];
  lowValue: PageRef[];
}

interface CrawledPage extends PageRef {
  content: string;
}

const MEDIA = [
  ".jpg",
  ".jpeg",
  ".png",
  ".gif",
  ".svg",
  ".webp",
  ".ico",
  ".pdf",
  ".mp4",
  ".mov",
  ".avi",
  ".wmv",
  ".flv",
  ".webm",
];

const PAGE_LIMITS = [10, 25, 50];

const PAGE = {
  type: "object",
  properties: { url: { type: "string" }, title: { type: "string" } },
  required: ["url", "title"],
};

const SCHEMA = {
  type: "object",
  properties: {
    highValue: {
      type: "array",
      description: "An array of high-value pages grouped by language.",
      items: {
        type: "object",
        properties: {
          language: { type: "string", description: "Language code (e.g., 'en', 'de')" },
          pages: { type: "array", items: PAGE },
        },
        required: ["language", "pages"],
      },
    },
    lowValue: { type: "array", items: PAGE },
  },
  required: ["highValue", "lowValue"],
};

/** The site URLs, one per line; throws on one that isn't an http(s) URL. */
function parseUrls(text: string): string[] {
  const out: string[] = [];
  for (const line of text.split("\n")) {
    const raw = line.trim();
    if (!raw) continue;
    let u: URL;
    try {
      u = new URL(raw);
    } catch {
      throw new Error(`“${raw}” isn't a valid URL (like https://example.com).`);
    }
    if (u.protocol !== "http:" && u.protocol !== "https:") {
      throw new Error(`“${raw}” isn't an http or https address.`);
    }
    if (!out.includes(u.href)) out.push(u.href);
  }
  return out;
}

function categorizePrompt(pages: PageRef[]): string {
  return `Categorize the following website pages into "highValue" (grouped by language) and "lowValue".
High value pages are those that contain core content, documentation, articles, or products.
Low value pages are those that are utility-based, like login, signup, privacy policy, terms of service, or search results.

Pages:
${JSON.stringify(pages)}`;
}

/** Keeps the model's grouping to pages that were crawled (with their real titles); pages it
 * left out become low value, so nothing crawled goes missing. */
function normalize(answer: Partial<ScanResult> | null, pages: PageRef[]): ScanResult {
  const known = new Map(pages.map((p) => [p.url, p]));
  const used = new Set<string>();
  const take = (p: unknown): PageRef | null => {
    const url = p && typeof p === "object" ? (p as { url?: unknown }).url : null;
    if (typeof url !== "string") return null;
    const page = known.get(url) ?? known.get(url.replace(/\/$/, "")) ?? known.get(`${url}/`);
    if (!page || used.has(page.url)) return null;
    used.add(page.url);
    return page;
  };
  const groups = answer?.highValue;
  const low = answer?.lowValue;
  const highValue: LanguageGroup[] = [];
  for (const g of Array.isArray(groups) ? groups : []) {
    const list = (Array.isArray(g?.pages) ? g.pages : []).map(take).filter(Boolean) as PageRef[];
    if (list.length === 0) continue;
    const language =
      typeof g.language === "string" && g.language.trim() ? g.language.trim() : "unknown";
    const same = highValue.find((h) => h.language === language);
    if (same) same.pages.push(...list);
    else highValue.push({ language, pages: list });
  }
  const lowValue = (Array.isArray(low) ? low : []).map(take).filter(Boolean) as PageRef[];
  for (const p of pages) if (!used.has(p.url)) lowValue.push(p);
  return { highValue, lowValue };
}

/** llms.txt as the original wrote it. */
function buildLlmsTxt(host: string, scan: ScanResult, markdownReady: boolean): string {
  let t = `# ${host}\n\n`;
  if (markdownReady) {
    t += `## Guidance\nTo fetch any page on this site in clean Markdown, use the Accept: text/markdown header.\n\n`;
  }
  for (const g of scan.highValue) {
    t += `## High Value Pages (${g.language})\n`;
    for (const p of g.pages) t += `- [${p.title}](${p.url})\n`;
    t += `\n`;
  }
  if (scan.lowValue.length > 0) {
    t += `\n## Low Value Pages\n`;
    for (const p of scan.lowValue) t += `- [${p.title}](${p.url})\n`;
  }
  return t;
}

/** llms-full.txt: every high-value page's content as Markdown. */
function buildLlmsFull(scan: ScanResult, pages: Map<string, CrawledPage>): string {
  let t = "";
  for (const g of scan.highValue) {
    for (const p of g.pages) {
      const content = pages.get(p.url)?.content.trim();
      t += `# ${p.title}\n\nSource: ${p.url}\n\n${content || "[Failed to fetch content]"}\n\n---\n\n`;
    }
  }
  return t;
}

type Status = "idle" | "scanning" | "triaging" | "generating" | "ready" | "error";

export default function App() {
  const [urls, setUrls] = useState("");
  const [markdownReady, setMarkdownReady] = useState(false);
  const [limit, setLimit] = useState(50);
  const [status, setStatus] = useState<Status>("idle");
  const [note, setNote] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [scan, setScan] = useState<ScanResult | null>(null);
  const [skipped, setSkipped] = useState(0);
  const [output, setOutput] = useState<{ llms: string; full: string; zip: Blob } | null>(null);
  const [show, setShow] = useState<"llms" | "full">("llms");
  const [copied, setCopied] = useState(false);
  const abort = useRef<AbortController | null>(null);

  const busy = status === "scanning" || status === "triaging" || status === "generating";

  const run = async () => {
    setError(null);
    let list: string[];
    try {
      list = parseUrls(urls);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setStatus("error");
      return;
    }
    if (list.length === 0) return;
    const controller = new AbortController();
    abort.current = controller;
    setScan(null);
    setOutput(null);
    setSkipped(0);
    try {
      // 1. Crawl each site.
      setStatus("scanning");
      const pages = new Map<string, CrawledPage>();
      let skippedCount = 0;
      for (const [i, start] of list.entries()) {
        setNote(
          `Crawling ${new URL(start).hostname}${list.length > 1 ? ` (${i + 1} of ${list.length})` : ""}…`,
        );
        const r = await web.crawl(start, {
          maxPages: limit,
          maxDepth: 5,
          signal: controller.signal,
        });
        skippedCount += r.skipped.length;
        for (const p of r.pages) {
          if (pages.has(p.url) || !p.content.trim()) continue;
          pages.set(p.url, { url: p.url, title: p.title?.trim() || p.url, content: p.content });
        }
      }
      setSkipped(skippedCount);
      const refs = [...pages.values()]
        .filter((p) => !MEDIA.some((ext) => new URL(p.url).pathname.toLowerCase().endsWith(ext)))
        .map(({ url, title }) => ({ url, title }));
      if (refs.length === 0) throw new Error("No pages found");

      // 2. Triage with the app's model.
      setStatus("triaging");
      setNote("AI is analyzing page value...");
      let answer: Partial<ScanResult> | null;
      try {
        answer = await ai.json<Partial<ScanResult>>({
          prompt: categorizePrompt(refs),
          schema: SCHEMA,
          signal: controller.signal,
        });
      } catch (e) {
        if (controller.signal.aborted) throw e;
        // As in the original: when the answer can't be read, every page counts.
        answer = { highValue: [{ language: "unknown", pages: refs }], lowValue: [] };
      }
      const result = normalize(answer, refs);
      setScan(result);

      // 3. Generate the files.
      setStatus("generating");
      setNote("Generating llms.txt and bundling content...");
      const host = new URL(list[0] as string).hostname;
      const llms = buildLlmsTxt(host, result, markdownReady);
      const full = buildLlmsFull(result, pages);
      const zip = await files.zip([
        { name: "llms.txt", data: llms },
        { name: "llms-full.txt", data: full },
      ]);
      setOutput({ llms, full, zip });
      setStatus("ready");
    } catch (e) {
      if (controller.signal.aborted) {
        setStatus("idle");
      } else {
        setError(e instanceof Error ? e.message : String(e));
        setStatus("error");
      }
    } finally {
      abort.current = null;
    }
  };

  const copy = async () => {
    if (!output) return;
    await app.copy(show === "llms" ? output.llms : output.full);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const highCount = scan?.highValue.reduce((n, g) => n + g.pages.length, 0) ?? 0;

  return (
    <div className="mx-auto max-w-4xl p-6">
      <header className="mb-8 text-center">
        <h1 className="mb-3 flex items-center justify-center gap-3 text-3xl font-bold text-neutral-900 dark:text-white">
          <span className="rounded-xl bg-neutral-900 p-2 text-white dark:bg-white dark:text-neutral-900">
            <Globe className="h-7 w-7" />
          </span>
          llms.txt Generator
        </h1>
        <p className="mx-auto max-w-2xl text-neutral-600 dark:text-neutral-400">
          Create AI-friendly documentation files for your website.
        </p>
      </header>

      <section className="space-y-6 rounded-2xl border border-neutral-200 bg-white p-6 shadow-sm dark:border-neutral-700 dark:bg-neutral-800">
        <div className="space-y-2">
          <label
            htmlFor="site-urls"
            className="block text-sm font-medium text-neutral-700 dark:text-neutral-300"
          >
            Website URLs (one per line)
          </label>
          <div className="flex flex-col gap-4 md:flex-row">
            <textarea
              id="site-urls"
              placeholder={"https://example.com\nhttps://example.de"}
              value={urls}
              onChange={(e) => setUrls(e.target.value)}
              className="h-32 flex-1 rounded-xl border border-neutral-200 bg-white px-4 py-3 text-neutral-900 outline-none placeholder:text-neutral-400 focus:border-neutral-900 focus:ring-2 focus:ring-neutral-900 dark:border-neutral-700 dark:bg-neutral-800 dark:text-white dark:placeholder:text-neutral-500 dark:focus:border-white dark:focus:ring-white"
              disabled={busy}
            />
            <div className="flex w-full flex-col justify-center gap-3 md:w-52">
              {busy ? (
                <button
                  type="button"
                  onClick={() => abort.current?.abort()}
                  className="flex h-14 items-center justify-center gap-2 rounded-xl border border-neutral-300 px-6 font-medium text-neutral-800 hover:bg-neutral-100 dark:border-neutral-600 dark:text-neutral-200 dark:hover:bg-neutral-700"
                >
                  <Square className="h-4 w-4" /> Stop
                </button>
              ) : (
                <button
                  type="button"
                  onClick={() => void run()}
                  disabled={!urls.trim()}
                  className="flex h-14 items-center justify-center gap-2 rounded-xl bg-neutral-900 px-6 font-medium text-white hover:bg-neutral-800 disabled:bg-neutral-400 dark:bg-white dark:text-neutral-900 dark:hover:bg-neutral-100 dark:disabled:bg-neutral-600"
                >
                  Generate Files
                </button>
              )}
              <label className="flex items-center justify-between gap-2 text-xs text-neutral-600 dark:text-neutral-400">
                Pages per site
                <select
                  aria-label="Pages per site"
                  value={limit}
                  onChange={(e) => setLimit(Number(e.target.value))}
                  disabled={busy}
                  className="rounded-lg border border-neutral-200 bg-white px-2 py-1 text-sm text-neutral-900 dark:border-neutral-700 dark:bg-neutral-800 dark:text-white"
                >
                  {PAGE_LIMITS.map((n) => (
                    <option key={n} value={n}>
                      {n}
                    </option>
                  ))}
                </select>
              </label>
            </div>
          </div>

          <div className="flex items-center gap-3 rounded-xl border border-neutral-100 bg-neutral-50 p-4 dark:border-neutral-700 dark:bg-neutral-800/50">
            <input
              type="checkbox"
              id="markdown-ready"
              checked={markdownReady}
              onChange={(e) => setMarkdownReady(e.target.checked)}
              className="h-5 w-5 rounded border-neutral-300 dark:border-neutral-600"
            />
            <label
              htmlFor="markdown-ready"
              className="cursor-pointer select-none text-sm text-neutral-600 dark:text-neutral-400"
            >
              <span className="flex items-center gap-1.5">
                <span className="font-medium text-neutral-900 dark:text-white">
                  Markdown Ready?
                </span>
                <span title="Check this if your website server or CMS is configured to return content in Markdown format when the client sends an Accept: text/markdown HTTP header.">
                  <HelpCircle className="h-4 w-4 cursor-help text-neutral-400" />
                </span>
              </span>
              <span className="mt-0.5 block text-xs">
                Check this if your site supports content negotiation (Accept: text/markdown HTTP
                header).
              </span>
            </label>
          </div>
        </div>

        {busy && (
          <div className="flex items-center gap-3 rounded-xl border border-neutral-100 bg-neutral-50 p-4 text-neutral-600 dark:border-neutral-700 dark:bg-neutral-800/50 dark:text-neutral-300">
            <Loader2 className="h-5 w-5 animate-spin" />
            <span className="font-medium">{note || "Crawling website structure..."}</span>
          </div>
        )}

        {status === "error" && error && (
          <div className="flex items-center gap-3 rounded-xl border border-red-100 bg-red-50 p-4 text-red-600 dark:border-red-900/50 dark:bg-red-900/20 dark:text-red-400">
            <AlertCircle className="h-5 w-5 shrink-0" />
            <span className="font-medium">{error}</span>
          </div>
        )}

        {status === "ready" && scan && output && (
          <div className="space-y-6">
            <div className="flex items-center gap-3 rounded-xl border border-emerald-100 bg-emerald-50 p-4 text-emerald-700 dark:border-emerald-900/50 dark:bg-emerald-900/20 dark:text-emerald-400">
              <CheckCircle className="h-6 w-6 shrink-0" />
              <div>
                <p className="text-lg font-bold">Analysis Complete!</p>
                <p className="text-sm opacity-90">
                  Your AI-compatibility files are ready: {highCount} high-value and{" "}
                  {scan.lowValue.length} low-value pages
                  {skipped > 0 ? `, ${skipped} skipped (robots.txt or errors)` : ""}.
                </p>
              </div>
            </div>

            <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
              <div className="rounded-xl border border-neutral-200 bg-neutral-50 p-4 dark:border-neutral-700 dark:bg-neutral-800/50">
                <h3 className="mb-2 flex items-center gap-2 font-semibold text-neutral-900 dark:text-white">
                  <FileText className="h-4 w-4" /> High Value Pages ({highCount})
                </h3>
                <div className="max-h-40 space-y-2 overflow-y-auto">
                  {scan.highValue.map((g) => (
                    <div key={g.language}>
                      <h4 className="text-xs font-bold uppercase text-neutral-500 dark:text-neutral-400">
                        {g.language}
                      </h4>
                      <ul className="space-y-1 text-sm text-neutral-600 dark:text-neutral-300">
                        {g.pages.map((p) => (
                          <li key={p.url} className="truncate" title={p.url}>
                            <ExternalLink className="mr-1 inline h-3 w-3" />
                            {p.title}
                          </li>
                        ))}
                      </ul>
                    </div>
                  ))}
                </div>
              </div>
              <div className="rounded-xl border border-neutral-200 bg-neutral-50 p-4 opacity-75 dark:border-neutral-700 dark:bg-neutral-800/50">
                <h3 className="mb-2 flex items-center gap-2 font-semibold text-neutral-600 dark:text-neutral-400">
                  <FileText className="h-4 w-4" /> Low Value Pages ({scan.lowValue.length})
                </h3>
                <ul className="max-h-40 space-y-1 overflow-y-auto text-sm text-neutral-500 dark:text-neutral-400">
                  {scan.lowValue.map((p) => (
                    <li key={p.url} className="truncate" title={p.url}>
                      {p.title}
                    </li>
                  ))}
                </ul>
              </div>
            </div>

            <div className="overflow-hidden rounded-xl border border-neutral-200 dark:border-neutral-700">
              <div className="flex items-center justify-between border-b border-neutral-200 bg-neutral-50 px-3 py-2 dark:border-neutral-700 dark:bg-neutral-900">
                <span className="flex gap-1">
                  {(["llms", "full"] as const).map((k) => (
                    <button
                      key={k}
                      type="button"
                      onClick={() => setShow(k)}
                      className={`rounded-md px-3 py-1 font-mono text-xs ${
                        show === k
                          ? "bg-white text-neutral-900 shadow-sm dark:bg-neutral-700 dark:text-white"
                          : "text-neutral-500 hover:text-neutral-800 dark:hover:text-neutral-200"
                      }`}
                    >
                      {k === "llms" ? "llms.txt" : "llms-full.txt"}
                    </button>
                  ))}
                </span>
                <button
                  type="button"
                  onClick={() => void copy()}
                  className="flex items-center gap-1.5 text-xs font-medium text-neutral-500 hover:text-neutral-800 dark:text-neutral-400 dark:hover:text-neutral-200"
                >
                  {copied ? <Check className="h-3.5 w-3.5" /> : <Copy className="h-3.5 w-3.5" />}
                  {copied ? "Copied" : "Copy"}
                </button>
              </div>
              <pre
                aria-label={show === "llms" ? "llms.txt" : "llms-full.txt"}
                className="max-h-72 overflow-auto whitespace-pre-wrap bg-white p-4 font-mono text-xs text-neutral-700 dark:bg-neutral-950 dark:text-neutral-300"
              >
                {show === "llms" ? output.llms : output.full}
              </pre>
            </div>

            <button
              type="button"
              onClick={() =>
                void files.save("llms-pack.zip", output.zip, { mediaType: "application/zip" })
              }
              className="flex w-full items-center justify-center gap-3 rounded-xl bg-neutral-900 py-4 text-lg font-bold text-white shadow-lg hover:bg-neutral-800 dark:bg-white dark:text-neutral-900 dark:hover:bg-neutral-100"
            >
              <Download className="h-6 w-6" /> Download .zip Bundle
            </button>
            <p className="text-center text-xs text-neutral-400 dark:text-neutral-500">
              Contains <code>llms.txt</code> and <code>llms-full.txt</code>
            </p>
          </div>
        )}
      </section>

      <div className="mt-10 grid gap-6 text-center text-neutral-600 md:grid-cols-3 dark:text-neutral-400">
        <div className="p-4">
          <h3 className="mb-2 font-bold text-neutral-900 dark:text-white">1. Scan</h3>
          <p className="text-sm">
            Oxide crawls your site, following its robots.txt, to map out its structure and content.
          </p>
        </div>
        <div className="p-4">
          <h3 className="mb-2 font-bold text-neutral-900 dark:text-white">2. Triage</h3>
          <p className="text-sm">
            The app's AI model identifies your most valuable content for LLMs.
          </p>
        </div>
        <div className="p-4">
          <h3 className="mb-2 font-bold text-neutral-900 dark:text-white">3. Generate</h3>
          <p className="text-sm">
            Get standardized llms.txt files that help AI agents read your site better.
          </p>
        </div>
      </div>

      <div className="mt-10 rounded-2xl border border-neutral-200 bg-white p-6 shadow-sm dark:border-neutral-700 dark:bg-neutral-800">
        <h2 className="mb-6 text-center text-2xl font-bold text-neutral-900 dark:text-white">
          Why use llms.txt?
        </h2>
        <div className="grid gap-8 md:grid-cols-2">
          <div>
            <h3 className="mb-2 font-bold text-neutral-900 dark:text-white">llms.txt</h3>
            <p className="text-sm text-neutral-600 dark:text-neutral-400">
              A concise, human-readable summary of your website's documentation or content, designed
              to help AI agents quickly understand your site's structure and navigate to key
              information.
            </p>
          </div>
          <div>
            <h3 className="mb-2 font-bold text-neutral-900 dark:text-white">llms-full.txt</h3>
            <p className="text-sm text-neutral-600 dark:text-neutral-400">
              The complete, structured Markdown representation of your site's content. This gives AI
              agents the full context they need for high-quality retrieval and answers.
            </p>
          </div>
        </div>
        <p className="mt-8 text-center text-sm text-neutral-500 dark:text-neutral-400">
          Upload llms.txt and llms-full.txt into your website's root folder.
        </p>
      </div>
    </div>
  );
}
