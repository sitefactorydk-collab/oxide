import { useEffect, useRef, useState } from "react";
import {
  AlertCircle,
  ArrowLeftRight,
  ArrowRight,
  Check,
  ChevronDown,
  Copy,
  Cpu,
  Download,
  FileText,
  Languages,
  Loader2,
  Pencil,
  Plus,
  Settings2,
  Sparkles,
  Square,
  Trash2,
  Upload,
  X,
} from "lucide-react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { ai, app, files, storage, type FileInput } from "oxide";

const LANGUAGES = [
  "English",
  "Chinese (Simplified)",
  "Danish",
  "Dutch",
  "Finnish",
  "French",
  "German",
  "Icelandic",
  "Italian",
  "Japanese",
  "Korean",
  "Norwegian",
  "Portuguese",
  "Spanish",
  "Swedish",
];

interface Instruction {
  id: string;
  name: string;
  description: string;
}

const DEFAULT_INSTRUCTIONS: Instruction[] = [
  {
    id: "standard",
    name: "Standard Professional",
    description: "Translate accurately and professionally, maintaining the original tone.",
  },
  {
    id: "casual",
    name: "Casual / Colloquial",
    description: "Translate into casual, conversational language. Use idioms where appropriate.",
  },
  {
    id: "summarized",
    name: "Summarized Translation",
    description:
      "Provide a concise summary of the content in the target language, rather than a word-for-word translation.",
  },
  {
    id: "technical",
    name: "Code/Technical Fix",
    description:
      "Translate technical documentation accurately. Do not translate variable names or code snippets.",
  },
];

const STORAGE_KEY = "instructions";
const MAX_BYTES = 10 * 1024 * 1024;
const ACCEPT = ".pdf,.txt,.html,.htm,.md,.xml,.json,.csv,.png,.jpg,.jpeg,.webp";
const IMAGE_TYPES: Record<string, string> = {
  png: "image/png",
  jpg: "image/jpeg",
  jpeg: "image/jpeg",
  webp: "image/webp",
};
const TEXT_EXTENSIONS = ["txt", "html", "htm", "md", "xml", "json", "csv"];

type Kind = "image" | "pdf" | "text";

function extension(name: string): string {
  return name.toLowerCase().split(".").pop() ?? "";
}

/** What kind of document a file is (by its type, else its extension); null if unsupported. */
function fileKind(file: File): Kind | null {
  const ext = extension(file.name);
  if (file.type.startsWith("image/") || ext in IMAGE_TYPES) return "image";
  if (file.type === "application/pdf" || ext === "pdf") return "pdf";
  if (file.type.startsWith("text/") || TEXT_EXTENSIONS.includes(ext)) return "text";
  return null;
}

/** Keeps single line breaks as breaks (like remark-breaks), outside code blocks. */
function withBreaks(md: string): string {
  let fence = false;
  const lines = md.split("\n");
  return lines
    .map((line, i) => {
      if (/^\s*(```|~~~)/.test(line)) {
        fence = !fence;
        return line;
      }
      const next = lines[i + 1];
      if (fence || next === undefined || next.trim() === "" || line.trim() === "") return line;
      return line.endsWith("  ") ? line : `${line}  `;
    })
    .join("\n");
}

/** The translation as a Rich Text file (headings, bold, italics and bullets kept). */
function toRtf(markdown: string): string {
  const escape = (s: string) =>
    Array.from(s)
      .map((ch) => {
        const code = ch.codePointAt(0) ?? 0;
        if (code > 0xffff) {
          // Outside the BMP: RTF wants the UTF-16 halves.
          return Array.from(ch.split(""), (u) => {
            const c = u.charCodeAt(0);
            return `\\u${c > 32767 ? c - 65536 : c}?`;
          }).join("");
        }
        if (code > 127) return `\\u${code > 32767 ? code - 65536 : code}?`;
        if (ch === "\\") return "\\\\";
        if (ch === "{") return "\\{";
        if (ch === "}") return "\\}";
        return ch;
      })
      .join("");
  let rtf =
    "{\\rtf1\\ansi\\ansicpg1252\\uc1\\deff0\\deflang1033{\\fonttbl{\\f0\\fnil\\fcharset0 Calibri;}}\n";
  rtf += "\\pard\\sa200\\sl276\\slmult1\\f0\\fs22\n";
  for (const raw of markdown.split("\n")) {
    let line = escape(raw);
    if (/^###\s+/.test(line)) line = `\\fs28\\b ${line.replace(/^###\s+/, "")}\\b0\\fs22`;
    else if (/^##\s+/.test(line)) line = `\\fs32\\b ${line.replace(/^##\s+/, "")}\\b0\\fs22`;
    else if (/^#\s+/.test(line)) line = `\\fs36\\b ${line.replace(/^#\s+/, "")}\\b0\\fs22`;
    line = line.replace(/\*\*(.*?)\*\*/g, "\\b $1\\b0 ");
    line = line.replace(/\*(.*?)\*/g, "\\i $1\\i0 ");
    if (/^[-*]\s+/.test(line)) line = `\\bullet  ${line.replace(/^[-*]\s+/, "")}`;
    rtf += `${line}\\par\n`;
  }
  return `${rtf}}`;
}

function commentsSection(commentsLanguage: string): string {
  return `**COMMENTS SECTION FORMAT**:
- Add a horizontal rule (---) before this section.
- Header: "💡 **Comments from the translator**" (translated into ${commentsLanguage}).
- Use bullet points for the comments.
- Write the comments in ${commentsLanguage}.`;
}

function layoutFormat(target: string, comments: string): string {
  return `OUTPUT FORMAT (Strict Markdown):
1. **Introduction**: Start with a brief sentence in ${comments} explaining the structure.
2. **Layout Sections**: Use **Bold Headers** for layout groups (e.g., "**Images top row (left to right)**"). Translate these headers into ${comments}.
3. **Items**: Use **Bold** for item identifiers (e.g., "**Image 1**").
4. **Sub-labels**: Use *Italics* for specific text locations (e.g., "*Text on image:*", "*Text under image:*"). Translate these labels into ${comments}.
5. **Content**: The actual translated text follows the sub-labels, in ${target}.

IMPORTANT: Ensure there is a blank line between every line of text to ensure proper formatting.

Example structure:
**Header in ${comments}**

**Image 1**

*Label in ${comments}:* Translated Text

*Label in ${comments}:* Translated Text

**COMMENTS SECTION**:
If there are typos in the source, ambiguities, or clarifications:
${commentsSection(comments).split("\n").slice(1).join("\n")}`;
}

/** The instruction to the model (the original's wording). */
function buildPrompt(o: {
  target: string;
  instruction: Instruction | undefined;
  style: string;
  extra: string;
  commentsLanguage: string;
  includeComments: boolean;
  source: { kind: "text"; text: string; name?: string } | { kind: "image" | "pdf" };
}): string {
  let p = `Translate the following content to ${o.target}. `;
  p += o.instruction
    ? `\nStyle/Instruction: ${o.instruction.description}\n`
    : `Use a ${o.style} style. `;
  if (o.extra.trim()) p += `\nAdditional instructions: ${o.extra.trim()}\n`;
  if (o.source.kind !== "text") return `${p}\n\n${layoutFormat(o.target, o.commentsLanguage)}`;
  const preserve =
    "CRITICAL: Preserve all original formatting, paragraph breaks, and newlines exactly as they appear in the source text.";
  if (o.includeComments) {
    p += `\nIMPORTANT: Output the translated text/HTML first. Then, add translator comments if there are typos in the source, ambiguities, or clarifications.

${commentsSection(o.commentsLanguage)}

${preserve} Do not include any other conversational text or markdown formatting blocks (like \`\`\`html) around the translated content.\n`;
  } else {
    p += `\nIMPORTANT: Output ONLY the translated text/HTML. ${preserve} Do not include any conversational text, explanations, comments, or markdown formatting blocks (like \`\`\`html). Just the raw translated content.\n`;
  }
  const label = o.source.name
    ? `Content to translate (${o.source.name}):`
    : "Content to translate:";
  return `${p}\n${label}\n${o.source.text}`;
}

function Select(props: {
  value: string;
  onChange: (v: string) => void;
  options: string[];
  label: string;
  className?: string;
}) {
  return (
    <span className="relative inline-flex items-center">
      <select
        aria-label={props.label}
        className={
          props.className ??
          "cursor-pointer appearance-none rounded-xl border border-neutral-200 bg-neutral-50 py-2 pl-4 pr-10 text-sm font-medium text-neutral-900 focus:border-neutral-900 focus:outline-none focus:ring-1 focus:ring-neutral-900 dark:border-neutral-800 dark:bg-neutral-900 dark:text-white dark:focus:border-white dark:focus:ring-white"
        }
        value={props.value}
        onChange={(e) => props.onChange(e.target.value)}
      >
        {props.options.map((o) => (
          <option key={o} value={o}>
            {o}
          </option>
        ))}
      </select>
      <ChevronDown className="pointer-events-none absolute right-3 h-4 w-4 text-neutral-500" />
    </span>
  );
}

export default function App() {
  const [tab, setTab] = useState<"text" | "document">("text");
  const [target, setTarget] = useState("English");
  const [style, setStyle] = useState(DEFAULT_INSTRUCTIONS[0]?.name ?? "");
  const [commentsLanguage, setCommentsLanguage] = useState("English");
  const [text, setText] = useState("");
  const [extra, setExtra] = useState("");
  const [file, setFile] = useState<File | null>(null);
  const [preview, setPreview] = useState<string | null>(null);
  const [includeComments, setIncludeComments] = useState(false);
  const [output, setOutput] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [model, setModel] = useState("");
  const [copied, setCopied] = useState(false);
  const [dragging, setDragging] = useState(false);
  const [instructions, setInstructions] = useState<Instruction[]>(DEFAULT_INSTRUCTIONS);
  const [manage, setManage] = useState(false);
  const [editing, setEditing] = useState<Instruction | "new" | null>(null);
  const [form, setForm] = useState({ name: "", description: "" });
  const abort = useRef<AbortController | null>(null);

  useEffect(() => {
    void storage
      .get<Instruction[]>(STORAGE_KEY)
      .then((saved) => {
        if (Array.isArray(saved) && saved.length > 0) {
          setInstructions(saved);
          setStyle((s) => (saved.some((i) => i.name === s) ? s : (saved[0]?.name ?? "")));
        }
      })
      .catch(() => undefined);
  }, []);

  const saveInstructions = (next: Instruction[]) => {
    setInstructions(next);
    void storage.set(STORAGE_KEY, next).catch(() => undefined);
  };

  const choose = (f: File | undefined) => {
    if (!f) return;
    const kind = fileKind(f);
    if (!kind) {
      setError("Use a PDF, TXT, HTML, Markdown, XML, JSON, CSV, PNG, JPG or WEBP file.");
      return;
    }
    if (f.size > MAX_BYTES) {
      setError("The file is larger than 10 MB.");
      return;
    }
    setError(null);
    setFile(f);
    if (preview) URL.revokeObjectURL(preview);
    setPreview(kind === "image" ? URL.createObjectURL(f) : null);
  };

  const pick = () => void files.pick({ accept: ACCEPT }).then((f) => choose(f[0]));

  const translate = async () => {
    if (tab === "text" ? !text.trim() : !file) return;
    setBusy(true);
    setError(null);
    setOutput("");
    setModel("");
    const controller = new AbortController();
    abort.current = controller;
    try {
      const instruction = instructions.find((i) => i.name === style);
      const base = { target, instruction, style, extra, commentsLanguage, includeComments };
      let prompt: string;
      const images: FileInput[] = [];
      const documents: FileInput[] = [];
      if (tab === "text") {
        prompt = buildPrompt({ ...base, source: { kind: "text", text } });
      } else {
        const f = file as File;
        const kind = fileKind(f);
        if (kind === "text") {
          // Text files are read here and translated like pasted text, with comments.
          prompt = buildPrompt({
            ...base,
            includeComments: true,
            source: { kind: "text", text: await files.readText(f), name: f.name },
          });
        } else {
          prompt = buildPrompt({ ...base, source: { kind: kind === "image" ? "image" : "pdf" } });
          const mediaType =
            kind === "image"
              ? f.type || IMAGE_TYPES[extension(f.name)] || "image/png"
              : "application/pdf";
          const input: FileInput = { mediaType, data: await files.toBase64(f), name: f.name };
          (kind === "image" ? images : documents).push(input);
        }
      }
      const result = await ai.text({
        prompt,
        images: images.length ? images : undefined,
        documents: documents.length ? documents : undefined,
        signal: controller.signal,
        onText: (_delta, all) => setOutput(all),
      });
      setOutput(result.text.trim());
      setModel(result.model);
      if (!result.text.trim()) setError("The model returned an empty translation. Try again.");
    } catch (e) {
      if (!controller.signal.aborted) setError(e instanceof Error ? e.message : String(e));
    } finally {
      abort.current = null;
      setBusy(false);
    }
  };

  const copy = async () => {
    if (!output) return;
    await app.copy(output);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const saveRtf = () => {
    const date = new Date().toISOString().split("T")[0];
    void files.save(`${target}-translation-${date}.rtf`, toRtf(output), {
      mediaType: "application/rtf",
    });
  };

  const startEdit = (i: Instruction | "new") => {
    setEditing(i);
    setForm(
      i === "new" ? { name: "", description: "" } : { name: i.name, description: i.description },
    );
  };

  const saveEdit = () => {
    const name = form.name.trim();
    const description = form.description.trim();
    if (!name || !description) return;
    if (editing === "new") {
      saveInstructions([...instructions, { id: Date.now().toString(), name, description }]);
      setStyle(name);
    } else if (editing) {
      saveInstructions(
        instructions.map((i) => (i.id === editing.id ? { ...i, name, description } : i)),
      );
      if (style === editing.name) setStyle(name);
    }
    setEditing(null);
  };

  const remove = (id: string) => {
    const gone = instructions.find((i) => i.id === id);
    const next = instructions.filter((i) => i.id !== id);
    saveInstructions(next);
    if (gone && style === gone.name) setStyle(next[0]?.name ?? "");
  };

  const ready = tab === "text" ? text.trim() !== "" : file !== null;
  const tabClass = (active: boolean) =>
    `flex flex-1 items-center justify-center gap-2 border-b-2 px-4 py-4 text-sm font-medium transition-colors ${
      active
        ? "border-neutral-900 text-neutral-900 dark:border-white dark:text-white"
        : "border-transparent text-neutral-500 hover:text-neutral-700 dark:text-neutral-400 dark:hover:text-neutral-300"
    }`;
  const field =
    "w-full rounded-xl border border-neutral-200 bg-neutral-50 p-3 text-sm text-neutral-800 placeholder:text-neutral-400 focus:border-neutral-900 focus:outline-none focus:ring-1 focus:ring-neutral-900 dark:border-neutral-800 dark:bg-neutral-900 dark:text-neutral-200 dark:focus:border-white dark:focus:ring-white";

  return (
    <div className="mx-auto max-w-6xl space-y-6 p-6">
      <header className="mb-8 text-center">
        <h1 className="mb-3 flex items-center justify-center gap-3 text-3xl font-bold text-neutral-900 dark:text-white">
          <span className="rounded-xl bg-neutral-900 p-2 text-white dark:bg-white dark:text-neutral-900">
            <Languages className="h-7 w-7" />
          </span>
          Multi Purpose AI Translator
        </h1>
        <p className="mx-auto max-w-2xl text-neutral-600 dark:text-neutral-400">
          Translate text, HTML, images, and PDF files.
        </p>
      </header>

      <div className="flex flex-wrap items-center justify-between gap-4 rounded-2xl border border-neutral-200 bg-white p-4 shadow-sm dark:border-neutral-800 dark:bg-neutral-950">
        <div className="flex flex-wrap items-center gap-6">
          <div className="flex items-center gap-3">
            <span className="flex items-center gap-1.5 text-xs font-semibold tracking-wider text-neutral-500 dark:text-neutral-400">
              TRANSLATE TO <ArrowLeftRight className="h-3 w-3" />
            </span>
            <Select label="Translate to" value={target} onChange={setTarget} options={LANGUAGES} />
          </div>
          <div className="hidden h-8 w-px bg-neutral-200 sm:block dark:bg-neutral-800" />
          <div className="flex items-center gap-3">
            <span className="text-xs font-semibold tracking-wider text-neutral-500 dark:text-neutral-400">
              STYLE
            </span>
            <Select
              label="Style"
              value={style}
              onChange={setStyle}
              options={instructions.map((i) => i.name)}
            />
            <button
              type="button"
              title="Manage styles"
              className="text-neutral-400 hover:text-neutral-700 dark:hover:text-neutral-200"
              onClick={() => setManage(true)}
            >
              <Settings2 className="h-5 w-5" />
            </button>
          </div>
        </div>
        {busy ? (
          <button
            type="button"
            className="flex items-center gap-2 rounded-xl border border-neutral-300 px-6 py-2.5 text-sm font-semibold text-neutral-800 hover:bg-neutral-100 dark:border-neutral-700 dark:text-neutral-200 dark:hover:bg-neutral-900"
            onClick={() => abort.current?.abort()}
          >
            <Square className="h-4 w-4" /> Stop
          </button>
        ) : (
          <button
            type="button"
            className="flex items-center gap-2 rounded-xl bg-neutral-900 px-6 py-2.5 text-sm font-semibold text-white hover:bg-neutral-800 disabled:opacity-50 dark:bg-white dark:text-neutral-900 dark:hover:bg-neutral-200"
            onClick={() => void translate()}
            disabled={!ready}
          >
            <Sparkles className="h-4 w-4" /> Translate
          </button>
        )}
      </div>

      {error && (
        <div className="flex items-start gap-3 rounded-xl border border-red-200 bg-red-50 p-4 text-sm font-medium text-red-800 dark:border-red-900/60 dark:bg-red-950/40 dark:text-red-300">
          <AlertCircle className="mt-0.5 h-5 w-5 shrink-0" />
          {error}
        </div>
      )}

      <div className="grid grid-cols-1 gap-6 lg:grid-cols-2">
        <section className="flex flex-col rounded-2xl border border-neutral-200 bg-white shadow-sm dark:border-neutral-800 dark:bg-neutral-950">
          <div className="flex border-b border-neutral-200 dark:border-neutral-800">
            <button
              type="button"
              className={tabClass(tab === "text")}
              onClick={() => setTab("text")}
            >
              <FileText className="h-4 w-4" /> Text / HTML
            </button>
            <button
              type="button"
              className={tabClass(tab === "document")}
              onClick={() => setTab("document")}
            >
              <Upload className="h-4 w-4" /> Document / Image
            </button>
          </div>

          <div className="flex flex-1 flex-col p-6">
            {tab === "text" ? (
              <div className="flex flex-1 flex-col">
                <textarea
                  className="min-h-[320px] flex-1 resize-none border-none bg-transparent text-base text-neutral-700 placeholder:text-neutral-400 focus:outline-none dark:text-neutral-300 dark:placeholder:text-neutral-600"
                  placeholder="Paste your text or HTML here..."
                  value={text}
                  onChange={(e) => setText(e.target.value)}
                />
                <div className="mt-4 flex flex-wrap items-center gap-4 border-t border-neutral-100 pt-4 dark:border-neutral-800">
                  <label className="flex cursor-pointer items-center gap-2 text-sm font-medium text-neutral-700 dark:text-neutral-300">
                    <input
                      type="checkbox"
                      className="h-4 w-4 rounded border-neutral-300 dark:border-neutral-700"
                      checked={includeComments}
                      onChange={(e) => setIncludeComments(e.target.checked)}
                    />
                    Include translator comments
                  </label>
                  {includeComments && (
                    <span className="flex items-center gap-2 text-sm">
                      <span className="text-neutral-500 dark:text-neutral-400">in</span>
                      <Select
                        label="Comments language"
                        value={commentsLanguage}
                        onChange={setCommentsLanguage}
                        options={LANGUAGES}
                        className="cursor-pointer appearance-none bg-transparent py-1 pl-2 pr-8 font-medium text-neutral-900 focus:outline-none dark:text-white"
                      />
                    </span>
                  )}
                </div>
              </div>
            ) : (
              <div className="flex flex-1 flex-col items-center justify-center rounded-2xl border-2 border-dashed border-neutral-200 bg-neutral-50/50 p-6 text-center dark:border-neutral-800 dark:bg-neutral-900/50">
                <div className="mb-6 flex items-center gap-3 rounded-xl border border-neutral-200 bg-white px-4 py-2 shadow-sm dark:border-neutral-800 dark:bg-neutral-950">
                  <span className="text-xs font-semibold tracking-wider text-neutral-500 dark:text-neutral-400">
                    COMMENTS IN
                  </span>
                  <Select
                    label="Comments language"
                    value={commentsLanguage}
                    onChange={setCommentsLanguage}
                    options={LANGUAGES}
                    className="cursor-pointer appearance-none bg-transparent py-1 pl-2 pr-8 text-sm font-medium text-neutral-900 focus:outline-none dark:text-white"
                  />
                </div>
                <div
                  role="button"
                  tabIndex={0}
                  onClick={pick}
                  onKeyDown={(e) => e.key === "Enter" && pick()}
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
                  className={`flex w-full cursor-pointer flex-col items-center justify-center rounded-xl p-8 transition-colors ${
                    dragging ? "bg-neutral-100 dark:bg-neutral-800/60" : ""
                  }`}
                >
                  {file ? (
                    <div className="flex flex-col items-center gap-4">
                      {preview ? (
                        <img
                          src={preview}
                          alt="Preview"
                          className="h-32 w-32 rounded-xl border border-neutral-200 object-cover shadow-sm dark:border-neutral-800"
                        />
                      ) : (
                        <span className="flex h-32 w-32 items-center justify-center rounded-xl border border-neutral-200 bg-neutral-100 shadow-sm dark:border-neutral-800 dark:bg-neutral-800">
                          <FileText className="h-12 w-12 text-neutral-400" />
                        </span>
                      )}
                      <span>
                        <span className="block max-w-[280px] truncate font-medium text-neutral-900 dark:text-white">
                          {file.name}
                        </span>
                        <span className="mt-1 block text-xs text-neutral-500 dark:text-neutral-400">
                          Click or drag to replace
                        </span>
                      </span>
                    </div>
                  ) : (
                    <>
                      <span className="mb-4 flex h-16 w-16 items-center justify-center rounded-full bg-neutral-100 text-neutral-900 dark:bg-neutral-800 dark:text-white">
                        <Upload className="h-8 w-8" />
                      </span>
                      <span className="mb-2 text-lg font-semibold text-neutral-900 dark:text-white">
                        Drag & drop a file here, or click to select
                      </span>
                      <span className="max-w-[280px] text-sm text-neutral-500 dark:text-neutral-400">
                        Supports PDF, TXT, HTML, PNG, JPG, WEBP
                      </span>
                    </>
                  )}
                </div>
              </div>
            )}

            <label className="mt-6 block">
              <span className="mb-2 block text-xs font-semibold tracking-wider text-neutral-500 dark:text-neutral-400">
                Additional Instructions
              </span>
              <textarea
                className={`${field} resize-none`}
                rows={3}
                placeholder='e.g., "Keep the tone professional" or "Translate only the headings"'
                value={extra}
                onChange={(e) => setExtra(e.target.value)}
              />
            </label>
          </div>

          <div className="flex items-center justify-between border-t border-neutral-200 bg-neutral-50/50 px-6 py-3 text-xs text-neutral-500 dark:border-neutral-800 dark:bg-neutral-900/50 dark:text-neutral-400">
            <span>
              {tab === "text"
                ? `${text.length.toLocaleString()} characters`
                : file
                  ? "1 selected file"
                  : "No file selected"}
            </span>
            <span className="flex items-center gap-1.5">
              <span className="h-1.5 w-1.5 rounded-full bg-neutral-900 dark:bg-white" />
              Using: {style}
            </span>
          </div>
        </section>

        <section className="flex flex-col rounded-2xl border border-neutral-200 bg-white shadow-sm dark:border-neutral-800 dark:bg-neutral-950">
          <div className="flex items-center justify-between border-b border-neutral-200 px-6 py-4 dark:border-neutral-800">
            <h2 className="flex items-center gap-2 font-semibold text-neutral-900 dark:text-white">
              <span className="h-2 w-2 rounded-full bg-emerald-500" /> Translated Output
            </h2>
            {output && !busy && (
              <div className="flex items-center gap-4">
                {tab === "document" && (
                  <button
                    type="button"
                    onClick={saveRtf}
                    className="flex items-center gap-1.5 text-sm font-medium text-neutral-500 hover:text-neutral-800 dark:text-neutral-400 dark:hover:text-neutral-200"
                  >
                    <Download className="h-4 w-4" /> Save as Rich Text file
                  </button>
                )}
                <button
                  type="button"
                  onClick={() => void copy()}
                  className="flex items-center gap-1.5 text-sm font-medium text-neutral-500 hover:text-neutral-800 dark:text-neutral-400 dark:hover:text-neutral-200"
                >
                  {copied ? (
                    <Check className="h-4 w-4 text-emerald-500" />
                  ) : (
                    <Copy className="h-4 w-4" />
                  )}
                  {copied ? "Copied!" : "Copy"}
                </button>
              </div>
            )}
          </div>

          <div className="flex flex-1 flex-col p-6">
            {output ? (
              <div className="flex-1 space-y-3 leading-relaxed text-neutral-700 dark:text-neutral-300 [&_a]:underline [&_code]:font-mono [&_h1]:text-xl [&_h1]:font-bold [&_h2]:text-lg [&_h2]:font-bold [&_h3]:font-semibold [&_hr]:my-4 [&_hr]:border-neutral-200 dark:[&_hr]:border-neutral-800 [&_li]:ml-5 [&_ol]:list-decimal [&_table]:w-full [&_td]:border [&_td]:border-neutral-200 [&_td]:p-2 dark:[&_td]:border-neutral-800 [&_th]:border [&_th]:border-neutral-200 [&_th]:p-2 dark:[&_th]:border-neutral-800 [&_ul]:list-disc">
                <ReactMarkdown remarkPlugins={[remarkGfm]}>{withBreaks(output)}</ReactMarkdown>
              </div>
            ) : busy ? (
              <div className="flex flex-1 flex-col items-center justify-center text-center">
                <Loader2 className="mb-4 h-8 w-8 animate-spin text-emerald-500" />
                <span className="text-lg font-semibold text-neutral-900 dark:text-white">
                  Translating...
                </span>
              </div>
            ) : (
              <div className="flex flex-1 flex-col items-center justify-center text-center">
                <span className="mb-6 flex h-20 w-20 items-center justify-center rounded-full bg-neutral-100 text-neutral-400 dark:bg-neutral-900 dark:text-neutral-500">
                  <ArrowRight className="h-8 w-8" />
                </span>
                <span className="mb-2 text-lg font-semibold text-neutral-400 dark:text-neutral-500">
                  Ready to translate
                </span>
                <span className="max-w-[240px] text-sm text-neutral-400/80 dark:text-neutral-500/80">
                  Select your content and click Translate to see the magic happen.
                </span>
              </div>
            )}
          </div>

          {model && (
            <div className="flex items-center gap-1.5 border-t border-neutral-200 px-6 py-3 font-mono text-xs text-neutral-500 dark:border-neutral-800 dark:text-neutral-400">
              <Cpu className="h-3.5 w-3.5" /> {model}
            </div>
          )}
        </section>
      </div>

      {manage && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-6">
          <div className="flex max-h-[85vh] w-full max-w-lg flex-col overflow-hidden rounded-2xl border border-neutral-200 bg-white shadow-2xl dark:border-neutral-800 dark:bg-neutral-950">
            <div className="flex items-center justify-between border-b border-neutral-200 px-6 py-4 dark:border-neutral-800">
              <h3 className="font-semibold text-neutral-900 dark:text-white">
                {editing === "new" ? "New style" : editing ? "Edit style" : "Translation styles"}
              </h3>
              <button
                type="button"
                aria-label="Close"
                className="p-1 text-neutral-400 hover:text-neutral-700 dark:hover:text-neutral-200"
                onClick={() => {
                  setManage(false);
                  setEditing(null);
                }}
              >
                <X className="h-5 w-5" />
              </button>
            </div>
            {editing ? (
              <div className="space-y-4 p-6">
                <label className="block space-y-1.5">
                  <span className="text-sm font-medium text-neutral-700 dark:text-neutral-300">
                    Name
                  </span>
                  <input
                    className={field}
                    value={form.name}
                    placeholder="e.g. Marketing copy"
                    onChange={(e) => setForm({ ...form, name: e.target.value })}
                  />
                </label>
                <label className="block space-y-1.5">
                  <span className="text-sm font-medium text-neutral-700 dark:text-neutral-300">
                    Instruction
                  </span>
                  <textarea
                    className={`${field} resize-y`}
                    rows={4}
                    value={form.description}
                    placeholder="How the model should translate in this style"
                    onChange={(e) => setForm({ ...form, description: e.target.value })}
                  />
                </label>
                <div className="flex justify-end gap-2">
                  <button
                    type="button"
                    className="rounded-lg px-4 py-2 text-sm font-medium text-neutral-600 hover:bg-neutral-100 dark:text-neutral-300 dark:hover:bg-neutral-900"
                    onClick={() => setEditing(null)}
                  >
                    Cancel
                  </button>
                  <button
                    type="button"
                    className="rounded-lg bg-neutral-900 px-4 py-2 text-sm font-semibold text-white disabled:opacity-50 dark:bg-white dark:text-neutral-900"
                    disabled={!form.name.trim() || !form.description.trim()}
                    onClick={saveEdit}
                  >
                    Save style
                  </button>
                </div>
              </div>
            ) : (
              <div className="flex-1 overflow-y-auto p-4">
                <ul className="space-y-2">
                  {instructions.map((i) => (
                    <li
                      key={i.id}
                      className="flex items-start justify-between gap-3 rounded-xl border border-neutral-200 p-3 dark:border-neutral-800"
                    >
                      <span>
                        <span className="block text-sm font-semibold text-neutral-900 dark:text-white">
                          {i.name}
                        </span>
                        <span className="block text-xs text-neutral-500 dark:text-neutral-400">
                          {i.description}
                        </span>
                      </span>
                      <span className="flex shrink-0 gap-1">
                        <button
                          type="button"
                          title={`Edit ${i.name}`}
                          className="rounded p-1.5 text-neutral-500 hover:bg-neutral-100 dark:hover:bg-neutral-900"
                          onClick={() => startEdit(i)}
                        >
                          <Pencil className="h-4 w-4" />
                        </button>
                        <button
                          type="button"
                          title={`Delete ${i.name}`}
                          className="rounded p-1.5 text-neutral-500 hover:bg-red-50 hover:text-red-600 disabled:opacity-40 dark:hover:bg-red-950/40"
                          disabled={instructions.length <= 1}
                          onClick={() => remove(i.id)}
                        >
                          <Trash2 className="h-4 w-4" />
                        </button>
                      </span>
                    </li>
                  ))}
                </ul>
                <button
                  type="button"
                  className="mt-4 flex w-full items-center justify-center gap-2 rounded-xl border border-dashed border-neutral-300 py-3 text-sm font-medium text-neutral-700 hover:bg-neutral-50 dark:border-neutral-700 dark:text-neutral-300 dark:hover:bg-neutral-900"
                  onClick={() => startEdit("new")}
                >
                  <Plus className="h-4 w-4" /> New style
                </button>
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
