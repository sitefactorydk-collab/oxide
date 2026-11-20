import { useState } from "react";
import {
  AlertCircle,
  BarChart3,
  BookOpen,
  CheckCircle2,
  Link as LinkIcon,
  Loader2,
  Sparkles,
  User,
  Zap,
} from "lucide-react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { ai, web } from "oxide";

interface Improvement {
  issue: string;
  suggestion: string;
}

interface Analysis {
  readabilityScore: number;
  gradeLevel: string;
  tone: string;
  targetAudience: string;
  strengths: string[];
  improvements: Improvement[];
  detailedAnalysis: string;
}

const SCHEMA = {
  type: "object",
  properties: {
    readabilityScore: { type: "number" },
    gradeLevel: { type: "string" },
    tone: { type: "string" },
    targetAudience: { type: "string" },
    strengths: { type: "array", items: { type: "string" } },
    improvements: {
      type: "array",
      items: {
        type: "object",
        properties: { issue: { type: "string" }, suggestion: { type: "string" } },
        required: ["issue", "suggestion"],
      },
    },
    detailedAnalysis: { type: "string", description: "Markdown" },
  },
  required: [
    "readabilityScore",
    "gradeLevel",
    "tone",
    "targetAudience",
    "strengths",
    "improvements",
    "detailedAnalysis",
  ],
};

function prompt(content: string, audience: string): string {
  return `You are an expert linguistic analyst and copywriter.
Analyze the following content for readability, tone and audience alignment.

${
  audience
    ? `Target audience (given by the user): ${audience}`
    : "Target audience: not given. Determine the likely audience from the content."
}

Content to analyze:
${content.substring(0, 25_000)}

Instructions:
1. Calculate a readability score (0-100, where 100 is easiest).
2. Determine the grade level (e.g. "6th Grade", "College Graduate").
3. Identify the primary tone (e.g. "Professional", "Conversational", "Technical", "Urgent").
4. If no target audience was given, identify the most likely one.
5. List 3 key strengths of the writing.
6. List 3 specific areas for improvement, each with a concrete suggestion for fixing it.
7. Give a detailed analysis of the readability and tone in Markdown.`;
}

export default function App() {
  const [mode, setMode] = useState<"url" | "text">("url");
  const [url, setUrl] = useState("");
  const [text, setText] = useState("");
  const [audience, setAudience] = useState("");
  const [busy, setBusy] = useState<"" | "fetching" | "analyzing">("");
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<Analysis | null>(null);

  const analyze = async () => {
    setError(null);
    setResult(null);
    try {
      let content = text;
      if (mode === "url") {
        try {
          new URL(url.trim());
        } catch {
          throw new Error("Enter a valid URL, like https://example.com");
        }
        setBusy("fetching");
        const page = await web.fetch(url.trim());
        content = page.content;
        if (content.trim().length < 100) {
          throw new Error("The page's text seems too short. Check the URL.");
        }
      }
      setBusy("analyzing");
      const a = await ai.json<Analysis>({
        prompt: prompt(content, audience.trim()),
        schema: SCHEMA,
      });
      setResult({
        ...a,
        readabilityScore: Math.max(0, Math.min(100, Math.round(Number(a.readabilityScore) || 0))),
        strengths: Array.isArray(a.strengths) ? a.strengths : [],
        improvements: Array.isArray(a.improvements) ? a.improvements : [],
      });
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy("");
    }
  };

  const tab = (active: boolean) =>
    `px-4 py-2 rounded-lg text-sm font-medium transition-all ${
      active
        ? "bg-white dark:bg-neutral-800 text-neutral-900 dark:text-white shadow-sm"
        : "text-neutral-500 hover:text-neutral-700 dark:hover:text-neutral-300"
    }`;
  const input =
    "w-full rounded-xl border border-neutral-200 dark:border-neutral-700 bg-white dark:bg-neutral-900 px-4 py-3 text-neutral-900 dark:text-white placeholder:text-neutral-400 focus:ring-2 focus:ring-neutral-900 dark:focus:ring-white outline-none";
  const card =
    "rounded-2xl border border-neutral-200 bg-white p-6 shadow-sm dark:border-neutral-800 dark:bg-neutral-900";
  const disabled = busy !== "" || (mode === "url" ? !url.trim() : !text.trim());

  return (
    <div className="mx-auto max-w-6xl p-6">
      <header className="mb-10 text-center">
        <h1 className="mb-3 flex items-center justify-center gap-3 text-3xl font-bold text-neutral-900 dark:text-white">
          <span className="rounded-xl bg-neutral-900 p-2 text-white dark:bg-white dark:text-neutral-900">
            <BookOpen className="h-7 w-7" />
          </span>
          Readability &amp; Tone Analyzer
        </h1>
        <p className="mx-auto max-w-2xl text-lg text-neutral-600 dark:text-neutral-400">
          Analyze content for readability, tone and audience alignment.
        </p>
      </header>

      <section className={`${card} mb-8 space-y-5 p-8`}>
        <div className="flex w-fit rounded-xl bg-neutral-100 p-1 dark:bg-neutral-950">
          <button type="button" className={tab(mode === "url")} onClick={() => setMode("url")}>
            Website URL
          </button>
          <button type="button" className={tab(mode === "text")} onClick={() => setMode("text")}>
            Direct text
          </button>
        </div>
        {mode === "url" ? (
          <label className="block space-y-2">
            <span className="flex items-center gap-2 text-sm font-medium text-neutral-700 dark:text-neutral-300">
              <LinkIcon className="h-4 w-4" /> Web page URL
            </span>
            <input
              type="url"
              className={input}
              placeholder="https://example.com/article"
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              disabled={busy !== ""}
            />
          </label>
        ) : (
          <label className="block space-y-2">
            <span className="text-sm font-medium text-neutral-700 dark:text-neutral-300">
              Text to analyze
            </span>
            <textarea
              className={`${input} h-48 resize-none`}
              placeholder="Paste your content here…"
              value={text}
              onChange={(e) => setText(e.target.value)}
              disabled={busy !== ""}
            />
          </label>
        )}
        <label className="block space-y-2">
          <span className="flex items-center gap-2 text-sm font-medium text-neutral-700 dark:text-neutral-300">
            <User className="h-4 w-4" /> Target audience (optional)
          </span>
          <input
            type="text"
            className={input}
            placeholder="e.g. Senior executives, high school students"
            value={audience}
            onChange={(e) => setAudience(e.target.value)}
            disabled={busy !== ""}
          />
        </label>
        <button
          type="button"
          onClick={() => void analyze()}
          disabled={disabled}
          className="flex w-full items-center justify-center gap-2 rounded-xl bg-neutral-900 py-4 font-bold text-white hover:bg-neutral-800 disabled:cursor-not-allowed disabled:bg-neutral-300 dark:bg-white dark:text-neutral-900 dark:disabled:bg-neutral-700"
        >
          {busy ? (
            <>
              <Loader2 className="h-5 w-5 animate-spin" />
              {busy === "fetching" ? "Reading the page…" : "Analyzing readability…"}
            </>
          ) : (
            <>
              <Sparkles className="h-5 w-5" /> Analyze content
            </>
          )}
        </button>
        {error && (
          <div className="flex items-start gap-3 rounded-xl border border-red-100 bg-red-50 p-4 text-sm font-medium text-red-800 dark:border-red-900/50 dark:bg-red-900/20 dark:text-red-300">
            <AlertCircle className="mt-0.5 h-5 w-5 shrink-0" />
            {error}
          </div>
        )}
      </section>

      {result && (
        <div className="space-y-8">
          <div className="grid grid-cols-1 gap-6 md:grid-cols-4">
            <div className={card}>
              <div className="mb-1 text-sm font-medium text-neutral-500">Readability score</div>
              <div className="text-3xl font-bold text-neutral-900 dark:text-white">
                {result.readabilityScore}/100
              </div>
              <div className="mt-2 h-2 w-full overflow-hidden rounded-full bg-neutral-100 dark:bg-neutral-700">
                <div
                  className="h-full bg-neutral-900 dark:bg-white"
                  style={{ width: `${result.readabilityScore}%` }}
                />
              </div>
            </div>
            {(
              [
                ["Grade level", result.gradeLevel],
                ["Primary tone", result.tone],
                ["Target audience", result.targetAudience],
              ] as const
            ).map(([label, value]) => (
              <div key={label} className={card}>
                <div className="mb-1 text-sm font-medium text-neutral-500">{label}</div>
                <div
                  className="truncate text-xl font-bold text-neutral-900 dark:text-white"
                  title={value}
                >
                  {value}
                </div>
              </div>
            ))}
          </div>
          <div className="grid grid-cols-1 gap-8 lg:grid-cols-2">
            <div className="space-y-6">
              <div className={card}>
                <h3 className="mb-5 flex items-center gap-2 text-lg font-bold text-neutral-900 dark:text-white">
                  <CheckCircle2 className="h-5 w-5 text-emerald-500" /> Key strengths
                </h3>
                <ul className="space-y-3">
                  {result.strengths.map((s, i) => (
                    <li
                      key={i}
                      className="flex items-start gap-3 text-neutral-700 dark:text-neutral-300"
                    >
                      <span className="mt-2 h-1.5 w-1.5 shrink-0 rounded-full bg-emerald-500" />
                      {s}
                    </li>
                  ))}
                </ul>
              </div>
              <div className={card}>
                <h3 className="mb-5 flex items-center gap-2 text-lg font-bold text-neutral-900 dark:text-white">
                  <Zap className="h-5 w-5 text-amber-500" /> Areas for improvement
                </h3>
                <div className="space-y-5">
                  {result.improvements.map((imp, i) => (
                    <div key={i} className="space-y-2">
                      <div className="flex items-start gap-3 font-semibold text-neutral-900 dark:text-white">
                        <span className="mt-2 h-1.5 w-1.5 shrink-0 rounded-full bg-amber-500" />
                        {imp.issue}
                      </div>
                      <p className="ml-4 rounded-lg border border-neutral-100 bg-neutral-50 p-3 text-sm text-neutral-600 dark:border-neutral-800 dark:bg-neutral-950 dark:text-neutral-400">
                        <strong className="text-neutral-900 dark:text-white">Suggestion:</strong>{" "}
                        {imp.suggestion}
                      </p>
                    </div>
                  ))}
                </div>
              </div>
            </div>
            <div className={card}>
              <h3 className="mb-5 flex items-center gap-2 text-lg font-bold text-neutral-900 dark:text-white">
                <BarChart3 className="h-5 w-5 text-blue-500" /> Detailed analysis
              </h3>
              <div className="space-y-3 text-sm leading-relaxed text-neutral-700 dark:text-neutral-300 [&_h1]:text-lg [&_h1]:font-bold [&_h2]:font-bold [&_h3]:font-semibold [&_li]:ml-5 [&_ol]:list-decimal [&_ul]:list-disc">
                <ReactMarkdown remarkPlugins={[remarkGfm]}>{result.detailedAnalysis}</ReactMarkdown>
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
