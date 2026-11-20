import { useState } from "react";
import {
  Check,
  Code,
  Copy,
  Download,
  Eye,
  Loader2,
  Sparkles,
  Table as TableIcon,
  Upload,
  X,
} from "lucide-react";
import { ai, app, files } from "oxide";

const IMAGE_TYPES = ["image/png", "image/jpeg", "image/webp"];
const MAX_BYTES = 10 * 1024 * 1024;

function instructions(styled: boolean): string {
  return `You are an expert web developer. Analyze the image of a table and convert it into clean, accurate HTML table code.

CRITICAL INSTRUCTIONS:
1. Output ONLY valid HTML <table> code. No Markdown code fences, explanations or other text.
2. Pay close attention to merged cells. Use colspan and rowspan accurately.
3. Pay close attention to text alignment. Use inline styles (e.g. style="text-align: right;") to match the image.
4. Distinguish table headers (<th>) from data cells (<td>).
${
  styled
    ? '5. Include basic inline CSS for borders, padding and layout so the table is readable (e.g. style="border-collapse: collapse; width: 100%;" on the table, and border and padding on cells).'
    : "5. DO NOT include inline CSS for borders, padding or layout. Output a bare table structure. The only inline CSS allowed is text alignment."
}
6. Preserve the text exactly as it appears in the image.`;
}

/** The table in a model's answer, without scripts or event handlers. */
function cleanTable(answer: string): string {
  const match = answer.match(/<table[\s\S]*?<\/table>/i);
  const raw = match
    ? match[0]
    : answer
        .replace(/^```(?:html|xml)?\s*/i, "")
        .replace(/\s*```$/, "")
        .trim();
  const doc = new DOMParser().parseFromString(`<body>${raw}</body>`, "text/html");
  doc.querySelectorAll("script, iframe, object, embed, link, style").forEach((n) => n.remove());
  doc.querySelectorAll("*").forEach((el) => {
    for (const attr of Array.from(el.attributes)) {
      if (attr.name.toLowerCase().startsWith("on")) el.removeAttribute(attr.name);
    }
  });
  return doc.body.innerHTML.trim();
}

export default function App() {
  const [image, setImage] = useState<File | null>(null);
  const [preview, setPreview] = useState<string | null>(null);
  const [styled, setStyled] = useState(false);
  const [html, setHtml] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [tab, setTab] = useState<"preview" | "code">("preview");
  const [copied, setCopied] = useState(false);
  const [dragging, setDragging] = useState(false);

  const choose = (file: File | undefined) => {
    if (!file) return;
    if (!IMAGE_TYPES.includes(file.type)) {
      setError("Use a PNG, JPEG or WebP image.");
      return;
    }
    if (file.size > MAX_BYTES) {
      setError("The image is larger than 10 MB.");
      return;
    }
    setError(null);
    setImage(file);
    setHtml("");
    if (preview) URL.revokeObjectURL(preview);
    setPreview(URL.createObjectURL(file));
  };

  const remove = () => {
    if (preview) URL.revokeObjectURL(preview);
    setImage(null);
    setPreview(null);
    setHtml("");
  };

  const generate = async () => {
    if (!image) return;
    setBusy(true);
    setError(null);
    setHtml("");
    try {
      const r = await ai.text({
        system: instructions(styled),
        prompt: "Please generate the HTML table code for this image.",
        images: [image],
      });
      const table = cleanTable(r.text);
      if (!table) throw new Error("The model returned no table. Try again or use another model.");
      setHtml(table);
      setTab("preview");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const copy = async () => {
    await app.copy(html);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const tabClass = (active: boolean) =>
    `flex items-center gap-2 rounded-lg px-4 py-2 text-sm font-medium ${
      active
        ? "bg-white text-neutral-900 shadow-sm dark:bg-neutral-800 dark:text-white"
        : "text-neutral-500 hover:text-neutral-900 dark:hover:text-white"
    }`;

  return (
    <div className="mx-auto max-w-6xl p-6">
      <header className="mb-8 text-center">
        <h1 className="mb-3 flex items-center justify-center gap-3 text-3xl font-bold text-neutral-900 dark:text-white">
          <span className="rounded-xl bg-neutral-900 p-2 text-white dark:bg-white dark:text-neutral-900">
            <TableIcon className="h-7 w-7" />
          </span>
          AI HTML Table from Image
        </h1>
        <p className="mx-auto max-w-2xl text-neutral-600 dark:text-neutral-400">
          Upload a picture of a table and get clean HTML, with merged cells and alignment kept.
        </p>
      </header>

      <div className="grid grid-cols-1 gap-8 lg:grid-cols-2">
        <section className="space-y-5 rounded-2xl border border-neutral-200 bg-white p-6 shadow-sm dark:border-neutral-800 dark:bg-neutral-900">
          <div
            role="button"
            tabIndex={0}
            onClick={() =>
              !image && void files.pick({ accept: IMAGE_TYPES.join(",") }).then((f) => choose(f[0]))
            }
            onKeyDown={(e) =>
              e.key === "Enter" &&
              !image &&
              void files.pick({ accept: IMAGE_TYPES.join(",") }).then((f) => choose(f[0]))
            }
            onDragOver={(e) => {
              e.preventDefault();
              setDragging(true);
            }}
            onDragLeave={() => setDragging(false)}
            onDrop={(e) => {
              e.preventDefault();
              setDragging(false);
              choose(e.dataTransfer.files[0]);
            }}
            className={`relative flex min-h-64 cursor-pointer flex-col items-center justify-center rounded-xl border-2 border-dashed p-6 text-center transition-colors ${
              dragging
                ? "border-blue-500 bg-blue-50 dark:bg-blue-950/30"
                : "border-neutral-300 hover:border-neutral-400 dark:border-neutral-700"
            }`}
          >
            {preview ? (
              <>
                <img
                  src={preview}
                  alt="The table to convert"
                  className="max-h-80 rounded-lg object-contain"
                />
                <button
                  type="button"
                  aria-label="Remove the image"
                  onClick={(e) => {
                    e.stopPropagation();
                    remove();
                  }}
                  className="absolute right-3 top-3 rounded-full bg-black/60 p-1.5 text-white hover:bg-black/80"
                >
                  <X className="h-4 w-4" />
                </button>
              </>
            ) : (
              <>
                <Upload className="mb-3 h-10 w-10 text-neutral-400" />
                <p className="font-medium text-neutral-700 dark:text-neutral-300">
                  Drop an image of a table here, or click to choose one
                </p>
                <p className="mt-1 text-xs text-neutral-500">PNG, JPEG or WebP, up to 10 MB</p>
              </>
            )}
          </div>
          <label className="flex items-center gap-3 text-sm text-neutral-700 dark:text-neutral-300">
            <input type="checkbox" checked={styled} onChange={(e) => setStyled(e.target.checked)} />
            Include basic inline styling (borders, padding)
          </label>
          <button
            type="button"
            disabled={!image || busy}
            onClick={() => void generate()}
            className="flex h-12 w-full items-center justify-center gap-2 rounded-xl bg-neutral-900 font-semibold text-white hover:bg-neutral-800 disabled:cursor-not-allowed disabled:bg-neutral-300 dark:bg-white dark:text-neutral-900 dark:disabled:bg-neutral-700"
          >
            {busy ? (
              <>
                <Loader2 className="h-5 w-5 animate-spin" /> Reading the table…
              </>
            ) : (
              <>
                <Sparkles className="h-5 w-5" /> Generate HTML table
              </>
            )}
          </button>
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
              <button
                type="button"
                disabled={!html}
                onClick={() => void copy()}
                className="flex items-center gap-1.5 rounded-lg border border-neutral-200 px-3 py-1.5 text-xs font-medium text-neutral-700 hover:bg-neutral-100 disabled:opacity-40 dark:border-neutral-700 dark:text-neutral-300 dark:hover:bg-neutral-800"
              >
                {copied ? (
                  <Check className="h-3.5 w-3.5 text-emerald-600" />
                ) : (
                  <Copy className="h-3.5 w-3.5" />
                )}
                {copied ? "Copied" : "Copy"}
              </button>
              <button
                type="button"
                disabled={!html}
                onClick={() => void files.save("table.html", html, { mediaType: "text/html" })}
                className="flex items-center gap-1.5 rounded-lg border border-neutral-200 px-3 py-1.5 text-xs font-medium text-neutral-700 hover:bg-neutral-100 disabled:opacity-40 dark:border-neutral-700 dark:text-neutral-300 dark:hover:bg-neutral-800"
              >
                <Download className="h-3.5 w-3.5" /> Download
              </button>
            </div>
          </div>
          <div className="flex-1 overflow-auto p-4">
            {!html ? (
              <div className="flex h-full items-center justify-center text-sm text-neutral-400">
                {busy ? "Working…" : "The table appears here."}
              </div>
            ) : tab === "preview" ? (
              <div
                className="text-sm text-neutral-900 dark:text-neutral-100 [&_td]:p-1 [&_th]:p-1"
                dangerouslySetInnerHTML={{ __html: html }}
              />
            ) : (
              <pre className="whitespace-pre-wrap font-mono text-xs text-neutral-800 dark:text-neutral-200">
                {html}
              </pre>
            )}
          </div>
        </section>
      </div>
    </div>
  );
}
