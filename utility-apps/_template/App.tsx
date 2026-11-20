import { useState } from "react";
import { Loader2, Sparkles } from "lucide-react";
import ReactMarkdown from "react-markdown";
import { ai, OxideError } from "oxide";

/** The starting point of a new app: ask the app's model something and show the answer. */
export default function App() {
  const [prompt, setPrompt] = useState("");
  const [answer, setAnswer] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const run = async () => {
    if (!prompt.trim() || busy) return;
    setBusy(true);
    setError(null);
    setAnswer("");
    try {
      await ai.text({ prompt, onText: (_delta, text) => setAnswer(text) });
    } catch (e) {
      setError(e instanceof OxideError ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="mx-auto flex max-w-2xl flex-col gap-4 p-6 text-neutral-900 dark:text-neutral-100">
      <h1 className="flex items-center gap-2 text-xl font-semibold">
        <Sparkles className="size-5 text-orange-600" /> New app
      </h1>
      <p className="text-sm text-neutral-500 dark:text-neutral-400">
        Describe what this app should do in the builder's chat, and it will be built here.
      </p>
      <textarea
        value={prompt}
        onChange={(e) => setPrompt(e.target.value)}
        rows={4}
        placeholder="Ask something…"
        className="w-full rounded-lg border border-neutral-300 bg-transparent p-3 text-sm dark:border-neutral-700"
      />
      <button
        type="button"
        onClick={() => void run()}
        disabled={busy || !prompt.trim()}
        className="inline-flex h-9 w-fit items-center gap-2 rounded-lg bg-neutral-900 px-4 text-sm font-medium text-white disabled:opacity-40 dark:bg-white dark:text-neutral-900"
      >
        {busy && <Loader2 className="size-4 animate-spin" />}
        Ask
      </button>
      {error && <p className="text-sm text-red-600">{error}</p>}
      {answer && (
        <div className="rounded-lg border border-neutral-200 p-4 text-sm leading-relaxed dark:border-neutral-800">
          <ReactMarkdown>{answer}</ReactMarkdown>
        </div>
      )}
    </div>
  );
}
