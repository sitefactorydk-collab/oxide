import { useMemo, useState, type ReactNode } from "react";
import {
  AlertCircle,
  Blocks,
  Check,
  CheckCircle,
  ChevronDown,
  ChevronRight,
  Code,
  Copy,
  Download,
  File as FileIcon,
  Folder,
  Loader2,
  Sparkles,
  Upload,
} from "lucide-react";
import JSZip from "jszip";
import { ai, app, files } from "oxide";

type Platform = "WordPress" | "Joomla" | "Drupal";
type JoomlaType = "Plugin" | "Module";

export interface PluginFile {
  path: string;
  content: string;
}

interface TreeNode {
  name: string;
  type: "file" | "folder";
  path: string;
  children: Record<string, TreeNode>;
}

const BINARY = /\.(png|jpg|jpeg|gif|ico|webp|zip|tar|gz|pdf|woff2?|ttf|eot|mo)$/i;
const BINARY_PLACEHOLDER = "[Binary File]";

const SCHEMA = {
  type: "object",
  properties: {
    files: {
      type: "array",
      items: {
        type: "object",
        properties: { path: { type: "string" }, content: { type: "string" } },
        required: ["path", "content"],
      },
    },
  },
  required: ["files"],
};

/** A relative path that stays inside the extension's folder (no `..`, no leading slash). */
function cleanPath(path: string): string {
  return path
    .replace(/\\/g, "/")
    .split("/")
    .filter((p) => p !== "" && p !== "." && p !== "..")
    .join("/");
}

/** The instruction to the model (the original's wording). */
function buildPrompt(o: {
  platform: Platform;
  joomlaType: JoomlaType;
  name: string;
  description: string;
  author: string;
  version: string;
  instructions: string;
  existing: PluginFile[] | null;
}): string {
  const existing = o.existing
    ? o.existing.map((f) => `--- FILE: ${f.path} ---\n${f.content}\n`).join("\n")
    : "";
  return `You are an expert CMS developer for ${o.platform}.
Task: ${o.existing ? "Modify the provided extension" : "Create a new installable extension"}.
Name: ${o.name}
Description: ${o.description}
Author: ${o.author}
Version: ${o.version}
${o.platform === "Joomla" ? `Extension Type: ${o.joomlaType}` : ""}

User Instructions: ${o.instructions}

${o.existing ? `Existing Files:\n${existing}\n\nModify these files or add new ones as requested.` : ""}

Return a JSON object containing a 'files' array. Each item must have 'path' (string, relative path including filename) and 'content' (string, the actual code).
Include all necessary core files (e.g., XML manifests, info.yml, main PHP files) required for a valid installable extension.
Do not use markdown formatting around the JSON.`;
}

async function readZip(zipFile: Blob): Promise<{ zip: JSZip; text: PluginFile[]; all: string[] }> {
  const zip = await JSZip.loadAsync(zipFile);
  const text: PluginFile[] = [];
  const all: string[] = [];
  for (const [path, entry] of Object.entries(zip.files)) {
    if (entry.dir) continue;
    all.push(path);
    if (!BINARY.test(path)) text.push({ path, content: await entry.async("string") });
  }
  return { zip, text, all };
}

function buildTree(list: PluginFile[]): TreeNode {
  const root: TreeNode = { name: "", type: "folder", path: "", children: {} };
  for (const f of list) {
    const parts = f.path.split("/").filter(Boolean);
    let node = root;
    parts.forEach((part, i) => {
      const path = parts.slice(0, i + 1).join("/");
      const last = i === parts.length - 1;
      node.children[part] ??= { name: part, type: last ? "file" : "folder", path, children: {} };
      node = node.children[part] as TreeNode;
    });
  }
  return root;
}

export default function App() {
  const [platform, setPlatform] = useState<Platform>("WordPress");
  const [joomlaType, setJoomlaType] = useState<JoomlaType>("Plugin");
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [author, setAuthor] = useState("");
  const [version, setVersion] = useState("1.0.0");
  const [instructions, setInstructions] = useState("");
  const [upload, setUpload] = useState<File | null>(null);
  const [status, setStatus] = useState<"idle" | "processing" | "generating" | "ready" | "error">(
    "idle",
  );
  const [error, setError] = useState<string | null>(null);
  const [generated, setGenerated] = useState<PluginFile[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const [dragging, setDragging] = useState(false);
  const [copied, setCopied] = useState(false);
  const [model, setModel] = useState("");

  const busy = status === "processing" || status === "generating";
  const tree = useMemo(() => buildTree(generated), [generated]);
  const current = generated.find((f) => f.path === selected) ?? null;

  const chooseZip = (f: File | undefined) => {
    if (!f) return;
    if (!/\.zip$/i.test(f.name)) {
      setError("Choose a .zip file.");
      return;
    }
    setError(null);
    setUpload(f);
  };

  const generate = async () => {
    if (!name.trim() || !instructions.trim()) {
      setError("Please provide at least a name and instructions.");
      return;
    }
    setStatus(upload ? "processing" : "generating");
    setError(null);
    setGenerated([]);
    setSelected(null);
    try {
      let original: Awaited<ReturnType<typeof readZip>> | null = null;
      if (upload) {
        original = await readZip(upload);
        setStatus("generating");
      }
      const answer = await ai.json<{ files?: PluginFile[] }>({
        prompt: buildPrompt({
          platform,
          joomlaType,
          name: name.trim(),
          description: description.trim(),
          author: author.trim(),
          version: version.trim(),
          instructions: instructions.trim(),
          existing: original?.text ?? null,
        }),
        schema: SCHEMA,
      });
      if (!Array.isArray(answer.files)) throw new Error("Invalid response format from AI.");
      const list: PluginFile[] = [];
      for (const f of answer.files) {
        const path = f && typeof f.path === "string" ? cleanPath(f.path) : "";
        if (!path || typeof f.content !== "string") continue;
        const at = list.findIndex((x) => x.path === path);
        if (at >= 0) list[at] = { path, content: f.content };
        else list.push({ path, content: f.content });
      }
      if (original) {
        // Files the model didn't return are kept as they were.
        for (const path of original.all) {
          if (list.some((f) => f.path === path)) continue;
          const text = original.text.find((f) => f.path === path);
          list.push({ path, content: text ? text.content : BINARY_PLACEHOLDER });
        }
      }
      if (list.length === 0) throw new Error("The model returned no files. Try again.");
      setGenerated(list);
      setSelected(list[0]?.path ?? null);
      setStatus("ready");
      const info = await ai.info().catch(() => ({ textModel: null }));
      setModel(info.textModel ?? "");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setStatus("error");
    }
  };

  const download = async () => {
    if (generated.length === 0) return;
    const zip = new JSZip();
    if (upload) {
      try {
        const original = await JSZip.loadAsync(upload);
        for (const [path, entry] of Object.entries(original.files)) {
          if (entry.dir) continue;
          const g = generated.find((f) => f.path === path);
          if (!g || g.content === BINARY_PLACEHOLDER) {
            zip.file(path, await entry.async("uint8array"));
          }
        }
      } catch (e) {
        setError(`The uploaded ZIP couldn't be read again: ${e instanceof Error ? e.message : e}`);
      }
    }
    for (const f of generated) {
      if (f.content !== BINARY_PLACEHOLDER) zip.file(f.path, f.content);
    }
    const bytes = await zip.generateAsync({ type: "uint8array" });
    const slug = name.trim().toLowerCase().replace(/\s+/g, "-") || "extension";
    await files.save(`${slug}-v${version.trim() || "1.0.0"}.zip`, bytes, {
      mediaType: "application/zip",
    });
  };

  const copy = async () => {
    if (!current) return;
    await app.copy(current.content);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const toggle = (path: string) => {
    const next = new Set(collapsed);
    if (next.has(path)) next.delete(path);
    else next.add(path);
    setCollapsed(next);
  };

  const renderNode = (node: TreeNode, depth: number): ReactNode => {
    const kids = Object.values(node.children).sort((a, b) =>
      a.type === b.type ? a.name.localeCompare(b.name) : a.type === "folder" ? -1 : 1,
    );
    if (node.type === "file") {
      return (
        <button
          type="button"
          key={node.path}
          title={node.path}
          className={`flex w-full items-center gap-2 rounded-md px-2 py-1 text-left text-sm ${
            selected === node.path
              ? "bg-neutral-200 font-medium text-neutral-900 dark:bg-neutral-800 dark:text-white"
              : "text-neutral-600 hover:bg-neutral-100 hover:text-neutral-900 dark:text-neutral-400 dark:hover:bg-neutral-800/50 dark:hover:text-neutral-200"
          }`}
          style={{ paddingLeft: `${depth * 12 + 8}px` }}
          onClick={() => setSelected(node.path)}
        >
          <FileIcon className="h-4 w-4 shrink-0 opacity-70" />
          <span className="truncate">{node.name}</span>
        </button>
      );
    }
    if (!node.path) return kids.map((k) => renderNode(k, depth));
    const open = !collapsed.has(node.path);
    return (
      <div key={node.path}>
        <button
          type="button"
          className="flex w-full items-center gap-1 rounded-md px-2 py-1 text-left text-sm text-neutral-700 hover:bg-neutral-100 dark:text-neutral-300 dark:hover:bg-neutral-800/50"
          style={{ paddingLeft: `${depth * 12 + 8}px` }}
          onClick={() => toggle(node.path)}
        >
          {open ? (
            <ChevronDown className="h-4 w-4 opacity-50" />
          ) : (
            <ChevronRight className="h-4 w-4 opacity-50" />
          )}
          <Folder className="h-4 w-4 text-blue-500 opacity-80" />
          <span className="truncate font-medium">{node.name}</span>
        </button>
        {open && kids.map((k) => renderNode(k, depth + 1))}
      </div>
    );
  };

  const input =
    "w-full rounded-xl border border-neutral-200 bg-white px-4 py-3 text-neutral-900 placeholder:text-neutral-400 outline-none focus:ring-2 focus:ring-neutral-900 disabled:opacity-60 dark:border-neutral-700 dark:bg-neutral-800 dark:text-white dark:focus:ring-white";
  const label = "block text-sm font-medium text-neutral-700 dark:text-neutral-300";

  return (
    <div className="mx-auto max-w-6xl p-6">
      <header className="mb-8 text-center">
        <h1 className="mb-3 flex items-center justify-center gap-3 text-3xl font-bold text-neutral-900 dark:text-white">
          <span className="rounded-xl bg-neutral-900 p-2 text-white dark:bg-white dark:text-neutral-900">
            <Blocks className="h-7 w-7" />
          </span>
          AI Plugin Creator
        </h1>
        <p className="mx-auto max-w-2xl text-neutral-600 dark:text-neutral-400">
          Generate custom plugins for WordPress, Joomla, and Drupal using AI.
        </p>
      </header>

      <section className="mb-8 rounded-2xl border border-neutral-200 bg-white p-6 shadow-sm dark:border-neutral-800 dark:bg-neutral-900">
        <div className="grid grid-cols-1 gap-8 lg:grid-cols-2">
          <div className="space-y-5">
            <label className="block space-y-2">
              <span className={label}>Platform</span>
              <select
                value={platform}
                onChange={(e) => setPlatform(e.target.value as Platform)}
                className={input}
                disabled={busy}
              >
                <option value="WordPress">WordPress</option>
                <option value="Joomla">Joomla</option>
                <option value="Drupal">Drupal</option>
              </select>
            </label>
            {platform === "Joomla" && (
              <label className="block space-y-2">
                <span className={label}>Extension Type</span>
                <select
                  value={joomlaType}
                  onChange={(e) => setJoomlaType(e.target.value as JoomlaType)}
                  className={input}
                  disabled={busy}
                >
                  <option value="Plugin">Plugin</option>
                  <option value="Module">Module</option>
                </select>
              </label>
            )}
            <div className="grid grid-cols-2 gap-4">
              <label className="block space-y-2">
                <span className={label}>Name</span>
                <input
                  className={input}
                  placeholder="e.g. Awesome SEO"
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                  disabled={busy}
                />
              </label>
              <label className="block space-y-2">
                <span className={label}>Version</span>
                <input
                  className={input}
                  placeholder="1.0.0"
                  value={version}
                  onChange={(e) => setVersion(e.target.value)}
                  disabled={busy}
                />
              </label>
            </div>
            <div className="grid grid-cols-2 gap-4">
              <label className="block space-y-2">
                <span className={label}>Author</span>
                <input
                  className={input}
                  placeholder="Your Name"
                  value={author}
                  onChange={(e) => setAuthor(e.target.value)}
                  disabled={busy}
                />
              </label>
              <label className="block space-y-2">
                <span className={label}>Description</span>
                <input
                  className={input}
                  placeholder="Brief description..."
                  value={description}
                  onChange={(e) => setDescription(e.target.value)}
                  disabled={busy}
                />
              </label>
            </div>
            <label className="block space-y-2">
              <span className={label}>Instructions / Prompt</span>
              <textarea
                className={`${input} h-32 resize-y`}
                placeholder="Describe what the extension should do in detail..."
                value={instructions}
                onChange={(e) => setInstructions(e.target.value)}
                disabled={busy}
              />
            </label>
          </div>

          <div className="space-y-6">
            <div className="space-y-2">
              <span className={label}>Modify Existing (Optional)</span>
              <div
                role="button"
                tabIndex={0}
                onClick={() =>
                  !busy &&
                  !upload &&
                  void files.pick({ accept: ".zip,application/zip" }).then((f) => chooseZip(f[0]))
                }
                onKeyDown={(e) =>
                  e.key === "Enter" &&
                  !busy &&
                  void files.pick({ accept: ".zip,application/zip" }).then((f) => chooseZip(f[0]))
                }
                onDragOver={(e) => {
                  e.preventDefault();
                  setDragging(true);
                }}
                onDragLeave={() => setDragging(false)}
                onDrop={(e) => {
                  e.preventDefault();
                  setDragging(false);
                  if (!busy) chooseZip(e.dataTransfer.files[0]);
                }}
                className={`flex h-32 cursor-pointer flex-col items-center justify-center rounded-xl border-2 border-dashed p-6 text-center transition-all ${
                  dragging
                    ? "border-emerald-500 bg-emerald-50 dark:bg-emerald-900/20"
                    : upload
                      ? "border-solid border-neutral-300 dark:border-neutral-600"
                      : "border-neutral-200 bg-neutral-50 hover:bg-neutral-100 dark:border-neutral-700 dark:bg-neutral-800/50 dark:hover:bg-neutral-800"
                }`}
              >
                {upload ? (
                  <>
                    <span className="mb-2 rounded-full bg-emerald-100 p-2 dark:bg-emerald-900/30">
                      <CheckCircle className="h-6 w-6 text-emerald-600 dark:text-emerald-400" />
                    </span>
                    <span className="max-w-[240px] truncate text-sm font-medium text-neutral-900 dark:text-white">
                      {upload.name}
                    </span>
                    <button
                      type="button"
                      className="mt-1 text-xs text-red-500 hover:text-red-600"
                      onClick={(e) => {
                        e.stopPropagation();
                        setUpload(null);
                      }}
                      disabled={busy}
                    >
                      Remove
                    </button>
                  </>
                ) : (
                  <>
                    <Upload className="mb-2 h-6 w-6 text-neutral-400" />
                    <span className="text-sm text-neutral-600 dark:text-neutral-400">
                      Drag & drop a ZIP file here
                    </span>
                    <span className="mt-1 text-xs text-neutral-400 dark:text-neutral-500">
                      or click to select
                    </span>
                  </>
                )}
              </div>
            </div>

            <button
              type="button"
              onClick={() => void generate()}
              disabled={!name.trim() || !instructions.trim() || busy}
              className="flex w-full items-center justify-center gap-3 rounded-xl bg-neutral-900 py-4 text-lg font-bold text-white shadow-sm hover:bg-neutral-800 disabled:bg-neutral-300 dark:bg-white dark:text-neutral-900 dark:hover:bg-neutral-100 dark:disabled:bg-neutral-700"
            >
              {busy ? (
                <>
                  <Loader2 className="h-6 w-6 animate-spin" />
                  {status === "processing"
                    ? "Reading ZIP & Analyzing..."
                    : "Generating Extension..."}
                </>
              ) : (
                <>
                  <Sparkles className="h-6 w-6" />
                  {upload ? "Modify Extension" : "Generate Extension"}
                </>
              )}
            </button>

            {error && (
              <div className="flex items-start gap-3 rounded-xl border border-red-100 bg-red-50 p-4 text-sm text-red-700 dark:border-red-900/50 dark:bg-red-900/20 dark:text-red-400">
                <AlertCircle className="h-5 w-5 shrink-0" />
                <p>{error}</p>
              </div>
            )}
          </div>
        </div>
      </section>

      {generated.length > 0 && (
        <section className="flex h-[600px] flex-col overflow-hidden rounded-2xl border border-neutral-200 bg-white shadow-sm dark:border-neutral-800 dark:bg-neutral-900">
          <div className="flex flex-wrap items-center justify-between gap-3 border-b border-neutral-200 bg-neutral-50 p-4 dark:border-neutral-800 dark:bg-neutral-950">
            <h3 className="flex items-center gap-2 font-semibold text-neutral-900 dark:text-white">
              <Code className="h-5 w-5 text-neutral-500 dark:text-neutral-400" /> Generated Files
              <span className="text-sm font-normal text-neutral-500">({generated.length})</span>
            </h3>
            <span className="flex items-center gap-3">
              {model && (
                <span className="rounded-md bg-neutral-100 px-3 py-1 font-mono text-xs text-neutral-500 dark:bg-neutral-800 dark:text-neutral-400">
                  {model}
                </span>
              )}
              <button
                type="button"
                onClick={() => void download()}
                className="flex items-center gap-2 rounded-lg bg-emerald-600 px-4 py-2 text-sm font-medium text-white shadow-sm hover:bg-emerald-700"
              >
                <Download className="h-4 w-4" /> Save to ZIP file
              </button>
            </span>
          </div>
          <div className="flex flex-1 overflow-hidden">
            <nav
              aria-label="Generated files"
              className="w-64 shrink-0 overflow-y-auto border-r border-neutral-200 bg-neutral-50/50 p-2 dark:border-neutral-800 dark:bg-neutral-900/50"
            >
              {renderNode(tree, 0)}
            </nav>
            <div className="flex flex-1 flex-col overflow-hidden bg-white dark:bg-[#1e1e1e]">
              {current ? (
                <>
                  <div className="flex items-center justify-between gap-2 border-b border-neutral-200 bg-neutral-50 px-4 py-2 font-mono text-sm text-neutral-600 dark:border-neutral-800 dark:bg-[#252526] dark:text-neutral-400">
                    <span className="flex items-center gap-2 truncate">
                      <FileIcon className="h-4 w-4 shrink-0" /> {current.path}
                    </span>
                    {current.content !== BINARY_PLACEHOLDER && (
                      <button
                        type="button"
                        title="Copy file"
                        onClick={() => void copy()}
                        className="rounded p-1 hover:bg-neutral-200 dark:hover:bg-neutral-700"
                      >
                        {copied ? (
                          <Check className="h-4 w-4 text-emerald-500" />
                        ) : (
                          <Copy className="h-4 w-4" />
                        )}
                      </button>
                    )}
                  </div>
                  <pre className="flex-1 overflow-auto whitespace-pre-wrap p-4 font-mono text-sm text-neutral-800 [line-break:anywhere] dark:text-neutral-300">
                    {current.content}
                  </pre>
                </>
              ) : (
                <div className="flex flex-1 items-center justify-center text-neutral-400 dark:text-neutral-600">
                  Select a file to view its contents
                </div>
              )}
            </div>
          </div>
        </section>
      )}
    </div>
  );
}
