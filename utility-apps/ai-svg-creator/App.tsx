import { useMemo, useState } from "react";
import {
  Check,
  Code,
  Copy,
  Download,
  Eye,
  ImagePlus,
  Loader2,
  PenTool,
  RotateCcw,
  Sparkles,
  X,
} from "lucide-react";
import { ai, app, files } from "oxide";

const VISION_TYPES = ["image/png", "image/jpeg", "image/webp"];

function instructions(animated: boolean): string {
  let s = `You are an expert SVG designer and developer. Generate professional, clean and scalable SVG code for the user's request.

CRITICAL INSTRUCTIONS:
1. Output ONLY valid SVG code. No Markdown code fences, explanations or other text.
2. The SVG must be self-contained and responsive (use viewBox rather than fixed width and height where appropriate, or standard sizes like 24x24 for icons).
3. Use semantic SVG elements (<path>, <circle>, <rect>, …).
4. Make the SVG accessible (add <title> and <desc> where appropriate).
5. Use clean formatting and indentation.`;
  if (animated) {
    s +=
      "\n6. The user wants an ANIMATED SVG. Use CSS animations (a <style> inside the SVG) or SMIL (<animate>, <animateTransform>, …) for smooth, professional motion. If the reference SVG already has animations, keep, fix or improve them as asked.";
  }
  return s;
}

/** The SVG in a model's answer. */
function extractSvg(answer: string): string {
  const match = answer.match(/<svg[\s\S]*?<\/svg>/i);
  if (match) return match[0];
  return answer
    .replace(/^```(?:xml|svg|html)?\s*/i, "")
    .replace(/\s*```$/, "")
    .trim();
}

export default function App() {
  const [prompt, setPrompt] = useState("");
  const [animated, setAnimated] = useState(false);
  const [reference, setReference] = useState<File | null>(null);
  const [referenceSvg, setReferenceSvg] = useState<string | null>(null);
  const [svg, setSvg] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [tab, setTab] = useState<"preview" | "code">("preview");
  const [copied, setCopied] = useState(false);

  // An <img> shows the SVG (animations play) without running anything in it.
  const previewUrl = useMemo(
    () => (svg ? `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}` : ""),
    [svg],
  );

  const chooseReference = async () => {
    const [file] = await files.pick({
      accept: [...VISION_TYPES, "image/svg+xml", ".svg"].join(","),
    });
    if (!file) return;
    setReference(file);
    if (file.type === "image/svg+xml" || file.name.toLowerCase().endsWith(".svg")) {
      const text = await file.text();
      setReferenceSvg(text);
      setSvg(text);
      setTab("preview");
    } else {
      setReferenceSvg(null);
    }
  };

  const clear = () => {
    setPrompt("");
    setSvg("");
    setReference(null);
    setReferenceSvg(null);
    setError(null);
  };

  const generate = async () => {
    if (!prompt.trim() && !reference) return;
    setBusy(true);
    setError(null);
    try {
      let text = prompt.trim() || "Recreate the reference image as a clean SVG.";
      if (svg) {
        text = `Here is the current SVG code (it may contain animations):
\`\`\`xml
${svg}
\`\`\`

Modify this SVG as follows. If the request is about animations, focus on the <style> block or the SMIL tags. Request:
${text}`;
      }
      if (referenceSvg && !svg.includes(referenceSvg)) {
        text += `\n\nReference SVG code from the user:\n\`\`\`xml\n${referenceSvg}\n\`\`\``;
      }
      const images = reference && VISION_TYPES.includes(reference.type) ? [reference] : [];
      const r = await ai.text({ system: instructions(animated), prompt: text, images });
      const out = extractSvg(r.text);
      if (!out.toLowerCase().includes("<svg")) {
        throw new Error("The model didn't return an SVG. Try again or rephrase the request.");
      }
      setSvg(out);
      setTab("preview");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const copy = async () => {
    await app.copy(svg);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const download = () => {
    const date = new Date().toISOString().split("T")[0];
    void files.save(`icon-${date}.svg`, svg, { mediaType: "image/svg+xml" });
  };

  const tabClass = (active: boolean) =>
    `flex items-center gap-2 rounded-lg px-4 py-2 text-sm font-medium ${
      active
        ? "bg-white text-neutral-900 shadow-sm dark:bg-neutral-800 dark:text-white"
        : "text-neutral-500 hover:text-neutral-900 dark:hover:text-white"
    }`;
  const small =
    "flex items-center gap-1.5 rounded-lg border border-neutral-200 px-3 py-1.5 text-xs font-medium text-neutral-700 hover:bg-neutral-100 disabled:opacity-40 dark:border-neutral-700 dark:text-neutral-300 dark:hover:bg-neutral-800";

  return (
    <div className="mx-auto max-w-6xl p-6">
      <header className="mb-8 text-center">
        <h1 className="mb-3 flex items-center justify-center gap-3 text-3xl font-bold text-neutral-900 dark:text-white">
          <span className="rounded-xl bg-neutral-900 p-2 text-white dark:bg-white dark:text-neutral-900">
            <PenTool className="h-7 w-7" />
          </span>
          AI SVG Creator
        </h1>
        <p className="mx-auto max-w-2xl text-neutral-600 dark:text-neutral-400">
          Describe an icon or graphic, add a reference image or SVG, and refine the result step by
          step.
        </p>
      </header>

      <div className="grid grid-cols-1 gap-8 lg:grid-cols-2">
        <section className="space-y-5 rounded-2xl border border-neutral-200 bg-white p-6 shadow-sm dark:border-neutral-800 dark:bg-neutral-900">
          <label className="block space-y-2">
            <span className="text-sm font-medium text-neutral-700 dark:text-neutral-300">
              {svg ? "What should change?" : "Describe the SVG"}
            </span>
            <textarea
              className="h-36 w-full resize-none rounded-xl border border-neutral-200 bg-white px-4 py-3 text-neutral-900 outline-none placeholder:text-neutral-400 focus:ring-2 focus:ring-neutral-900 dark:border-neutral-700 dark:bg-neutral-950 dark:text-white dark:focus:ring-white"
              placeholder={
                svg
                  ? "e.g. make the lines thicker and the colour teal"
                  : "e.g. a minimal rocket icon with a flame, line style"
              }
              value={prompt}
              onChange={(e) => setPrompt(e.target.value)}
              disabled={busy}
            />
          </label>
          <div className="flex flex-wrap items-center gap-3">
            {reference ? (
              <span className="inline-flex items-center gap-2 rounded-lg bg-neutral-100 px-3 py-1.5 text-xs text-neutral-700 dark:bg-neutral-800 dark:text-neutral-300">
                <ImagePlus className="h-3.5 w-3.5" /> {reference.name}
                <button
                  type="button"
                  aria-label="Remove the reference"
                  onClick={() => {
                    setReference(null);
                    setReferenceSvg(null);
                  }}
                >
                  <X className="h-3.5 w-3.5" />
                </button>
              </span>
            ) : (
              <button
                type="button"
                className={small}
                onClick={() => void chooseReference()}
                disabled={busy}
              >
                <ImagePlus className="h-3.5 w-3.5" /> Add a reference image or SVG
              </button>
            )}
            <label className="flex items-center gap-2 text-sm text-neutral-700 dark:text-neutral-300">
              <input
                type="checkbox"
                checked={animated}
                onChange={(e) => setAnimated(e.target.checked)}
              />
              Animated
            </label>
          </div>
          <div className="flex gap-3">
            <button
              type="button"
              disabled={busy || (!prompt.trim() && !reference)}
              onClick={() => void generate()}
              className="flex h-12 flex-1 items-center justify-center gap-2 rounded-xl bg-neutral-900 font-semibold text-white hover:bg-neutral-800 disabled:cursor-not-allowed disabled:bg-neutral-300 dark:bg-white dark:text-neutral-900 dark:disabled:bg-neutral-700"
            >
              {busy ? (
                <>
                  <Loader2 className="h-5 w-5 animate-spin" /> Drawing…
                </>
              ) : (
                <>
                  <Sparkles className="h-5 w-5" /> {svg ? "Update SVG" : "Generate SVG"}
                </>
              )}
            </button>
            <button type="button" className={small} onClick={clear} disabled={busy}>
              <RotateCcw className="h-3.5 w-3.5" /> Start over
            </button>
          </div>
          {error && <p className="text-sm font-medium text-red-600 dark:text-red-400">{error}</p>}
        </section>

        <section className="flex min-h-96 flex-col rounded-2xl border border-neutral-200 bg-white shadow-sm dark:border-neutral-800 dark:bg-neutral-900">
          <div className="flex items-center justify-between border-b border-neutral-200 p-3 dark:border-neutral-800">
            <div className="flex rounded-xl bg-neutral-100 p-1 dark:bg-neutral-950">
              <button
                type="button"
                className={tabClass(tab === "preview")}
                onClick={() => setTab("preview")}
              >
                <Eye className="h-4 w-4" /> Preview
              </button>
              <button
                type="button"
                className={tabClass(tab === "code")}
                onClick={() => setTab("code")}
              >
                <Code className="h-4 w-4" /> Code
              </button>
            </div>
            <div className="flex gap-2">
              <button type="button" className={small} disabled={!svg} onClick={() => void copy()}>
                {copied ? (
                  <Check className="h-3.5 w-3.5 text-emerald-600" />
                ) : (
                  <Copy className="h-3.5 w-3.5" />
                )}
                {copied ? "Copied" : "Copy"}
              </button>
              <button type="button" className={small} disabled={!svg} onClick={download}>
                <Download className="h-3.5 w-3.5" /> Download
              </button>
            </div>
          </div>
          <div className="flex flex-1 items-center justify-center overflow-auto p-6">
            {!svg ? (
              <span className="text-sm text-neutral-400">
                {busy ? "Working…" : "The SVG appears here."}
              </span>
            ) : tab === "preview" ? (
              <img
                src={previewUrl}
                alt="The generated SVG"
                className="max-h-96 w-full max-w-md object-contain [background:repeating-conic-gradient(#e5e5e5_0_25%,transparent_0_50%)_0_0/16px_16px]"
              />
            ) : (
              <pre className="w-full self-start whitespace-pre-wrap font-mono text-xs text-neutral-800 dark:text-neutral-200">
                {svg}
              </pre>
            )}
          </div>
        </section>
      </div>
    </div>
  );
}
