// The shared workbench of the SP Page Builder Addon Creator and the Elementor Widget Creator:
// a browser of the plugins in the app's folder (edit, clone, download, delete, import a ZIP,
// make a new one), and an editor with the plugin's file tree, a code window and an AI panel
// that changes the plugin from a request and screenshots. What differs between the two apps
// (prompts, packaging, checks) comes in as a `Platform` from App.tsx.
//
// Both apps carry an identical copy of this file (a test checks that they match).
import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import {
  AlertCircle,
  AlertTriangle,
  ArrowLeft,
  Check,
  ChevronDown,
  ChevronRight,
  Copy,
  Download,
  Edit3,
  File as FileIcon,
  FilePlus,
  Folder as FolderIcon,
  FolderOpen,
  FolderPlus,
  ImagePlus,
  Loader2,
  Plus,
  RefreshCw,
  Save,
  Search,
  Settings as SettingsIcon,
  Sparkles,
  Square,
  Trash2,
  Undo2,
  Upload,
  X,
  type LucideIcon,
} from "lucide-react";
import JSZip from "jszip";
import { ai, app, files, folder, storage, type FolderEntry, type FolderInfo } from "oxide";

// ------------------------------------------------------------------ types

/** The user's defaults, kept in the app's storage. */
export interface Settings {
  author: string;
  authorEmail: string;
  authorUrl: string;
  /** The category (SP Page Builder) or widget category (Elementor) new items go into. */
  category: string;
  /** SP Page Builder: the PHP vendor namespace (`C10`); Elementor: unused. */
  vendor: string;
  /** Added to every request to the model. */
  instructions: string;
}

/** A text file of a plugin (`content`), or a binary one (`content: null`). */
export interface PluginFile {
  path: string;
  content: string | null;
  size: number;
}

/** What the browser shows of a plugin. */
export interface PluginMeta {
  name: string;
  version: string;
  description: string;
  /** The plugin's slug (element name, text domain), from its files or its folder. */
  slug: string;
  /** It looks like a plugin of this kind. */
  recognized: boolean;
}

export interface Platform {
  /** The app's title and one-line description. */
  title: string;
  tagline: string;
  Icon: LucideIcon;
  /** "addon" / "widget". */
  noun: string;
  nounPlural: string;
  /** The ZIP installs in … ("Joomla", "WordPress"). */
  installsIn: string;
  /** A slug from a name (`c10faq`, `c10-faq`); empty when the name has no usable letters. */
  slugify(name: string): string;
  /** Why a slug isn't usable, or null. */
  slugProblem(slug: string): string | null;
  /** The plugin's folder name for a slug (`plg_sppagebuilder_c10faq`, `c10-faq`). */
  folderName(slug: string): string;
  /** The files (paths inside the plugin, up to three levels deep) worth reading to
   * describe it in the browser. */
  metaFiles(paths: string[]): string[];
  /** The plugin's name, version, slug … from those files. */
  describe(folderName: string, files: PluginFile[]): PluginMeta;
  /** Problems an installer or the page builder would trip on. */
  check(meta: PluginMeta, files: PluginFile[]): string[];
  /** The forms of a slug used in identifiers, replaced one for one when cloning. */
  slugForms(slug: string): string[];
  /** Instructions for the model. */
  system(settings: Settings): string;
  /** The request for a new plugin. */
  newPrompt(o: { name: string; slug: string; request: string; settings: Settings }): string;
  /** The request to change a plugin. */
  changePrompt(o: { meta: PluginMeta; files: PluginFile[]; request: string }): string;
  /** The folder inside the ZIP and the ZIP's name. */
  zipRoot(meta: PluginMeta, folderName: string): string;
  zipName(meta: PluginMeta, folderName: string): string;
  /** The folder an imported plugin goes into (from the ZIP's top folder and files). */
  importFolder(topFolder: string | null, files: PluginFile[]): string | null;
  /** Ideas shown under the new-plugin request. */
  ideas: string[];
  /** Labels of the settings that differ between the platforms. */
  categoryLabel: string;
  categoryHelp: string;
  vendorLabel: string | null;
}

interface Item {
  folderName: string;
  meta: PluginMeta;
}

interface HistoryEntry {
  at: number;
  request: string;
  summary: string;
  changed: string[];
}

interface AiAnswer {
  summary?: string;
  files?: Array<{ path?: unknown; content?: unknown }>;
  delete?: unknown[];
}

/** One AI change, so it can be undone: each file's content before (`null`: it didn't exist). */
interface Undo {
  folderName: string;
  before: Array<{ path: string; content: string | null }>;
}

type Modal =
  | { kind: "new" }
  | { kind: "clone"; item: Item }
  | { kind: "delete"; item: Item }
  | { kind: "settings" }
  | { kind: "replace"; folderName: string; files: ImportFile[] }
  | { kind: "leave"; then: () => void }
  | { kind: "newFile"; dir: boolean }
  | { kind: "renameFile"; path: string }
  | { kind: "deleteFile"; path: string };

interface ImportFile {
  path: string;
  data: Uint8Array;
}

// ------------------------------------------------------------------ helpers

const TEXT_FILE =
  /\.(php|xml|ini|js|mjs|ts|css|scss|less|json|md|txt|html?|svg|twig|ya?ml|pot?|csv|htaccess|sql|lock)$/i;
const MAX_TEXT_BYTES = 1024 * 1024;
const MAX_PROMPT_FILE = 150_000;
const SKIPPED = /(^|\/)(__MACOSX|\.DS_Store|Thumbs\.db|\.git)(\/|$)/;

const DEFAULT_SETTINGS: Settings = {
  author: "",
  authorEmail: "",
  authorUrl: "",
  category: "",
  vendor: "",
  instructions: "",
};

const ANSWER_SCHEMA = {
  type: "object",
  properties: {
    summary: { type: "string" },
    files: {
      type: "array",
      items: {
        type: "object",
        properties: { path: { type: "string" }, content: { type: "string" } },
        required: ["path", "content"],
      },
    },
    delete: { type: "array", items: { type: "string" } },
  },
  required: ["summary", "files"],
};

function message(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

/** A relative path that stays inside the plugin's folder, or "" (no `..`, no leading `/`). */
function cleanPath(path: string): string {
  return path
    .replace(/\\/g, "/")
    .split("/")
    .filter((p) => p !== "" && p !== "." && p !== "..")
    .join("/");
}

function isText(path: string, size: number): boolean {
  return TEXT_FILE.test(path) && size <= MAX_TEXT_BYTES;
}

function escapeRegExp(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/** Replaces each form of the old slug (and the old name) with the new one. */
function renamer(
  oldForms: string[],
  newForms: string[],
  oldName: string,
  newName: string,
): (text: string) => string {
  const pairs = oldForms
    .map((f, i) => [f, newForms[i] ?? f] as const)
    .filter(([a, b]) => a !== "" && a !== b)
    // Longer forms first, so `c10_faq` goes before `c10`.
    .sort((a, b) => b[0].length - a[0].length);
  return (text: string) => {
    let out = text;
    if (oldName.trim() && oldName !== newName) out = out.split(oldName).join(newName);
    for (const [a, b] of pairs) {
      const lower = a[0] === a[0]?.toLowerCase();
      // Lower-case forms stand alone (`c10faq__x`, `plg_sppagebuilder_c10faq`); others may
      // follow a prefix (`SppagebuilderAddonC10faq`).
      const left = lower ? "(?<![A-Za-z0-9])" : "";
      const right = a === a.toUpperCase() ? "(?![A-Z0-9])" : "(?![a-z0-9])";
      out = out.replace(new RegExp(`${left}${escapeRegExp(a)}${right}`, "g"), b);
    }
    return out;
  };
}

/** Bytes as UTF-8 text, or null when they aren't text. */
function bytesToText(data: Uint8Array): string | null {
  try {
    return new TextDecoder("utf-8", { fatal: true }).decode(data);
  } catch {
    return null;
  }
}

function formatSize(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}

function ago(ms: number): string {
  const s = Math.max(0, Math.round((Date.now() - ms) / 1000));
  if (s < 60) return "just now";
  if (s < 3600) return `${Math.round(s / 60)} min ago`;
  if (s < 86400) return `${Math.round(s / 3600)} h ago`;
  return new Date(ms).toLocaleDateString();
}

interface TreeNode {
  name: string;
  path: string;
  dir: boolean;
  children: Map<string, TreeNode>;
}

function buildTree(paths: string[], dirs: string[]): TreeNode {
  const root: TreeNode = { name: "", path: "", dir: true, children: new Map() };
  const add = (path: string, dir: boolean) => {
    const parts = path.split("/").filter(Boolean);
    let node = root;
    parts.forEach((part, i) => {
      const p = parts.slice(0, i + 1).join("/");
      const last = i === parts.length - 1;
      let next = node.children.get(part);
      if (!next) {
        next = { name: part, path: p, dir: last ? dir : true, children: new Map() };
        node.children.set(part, next);
      }
      node = next;
    });
  };
  for (const d of dirs) add(d, true);
  for (const p of paths) add(p, false);
  return root;
}

// ------------------------------------------------------------------ small UI pieces

const btn =
  "inline-flex items-center justify-center gap-1.5 rounded-lg border border-neutral-200 bg-white px-3 py-1.5 text-sm font-medium text-neutral-700 hover:bg-neutral-100 disabled:cursor-not-allowed disabled:opacity-50 dark:border-neutral-700 dark:bg-neutral-800 dark:text-neutral-200 dark:hover:bg-neutral-700";
const primary =
  "inline-flex items-center justify-center gap-1.5 rounded-lg bg-neutral-900 px-3 py-1.5 text-sm font-medium text-white hover:bg-neutral-800 disabled:cursor-not-allowed disabled:opacity-50 dark:bg-white dark:text-neutral-900 dark:hover:bg-neutral-200";
const danger =
  "inline-flex items-center justify-center gap-1.5 rounded-lg bg-red-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-red-700 disabled:opacity-50";
const input =
  "w-full rounded-lg border border-neutral-200 bg-white px-3 py-2 text-sm text-neutral-900 placeholder:text-neutral-400 outline-none focus:ring-2 focus:ring-neutral-900 disabled:opacity-60 dark:border-neutral-700 dark:bg-neutral-800 dark:text-white dark:focus:ring-white";
const iconBtn =
  "rounded-md p-1.5 text-neutral-500 hover:bg-neutral-200 hover:text-neutral-900 disabled:opacity-40 dark:text-neutral-400 dark:hover:bg-neutral-700 dark:hover:text-white";

function Dialog({
  title,
  children,
  onClose,
  wide,
}: {
  title: string;
  children: ReactNode;
  onClose: () => void;
  wide?: boolean;
}) {
  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center overflow-y-auto bg-black/40 p-6"
      onMouseDown={(e) => e.target === e.currentTarget && onClose()}
    >
      <div
        role="dialog"
        aria-label={title}
        className={`mt-10 w-full ${wide ? "max-w-2xl" : "max-w-md"} rounded-2xl border border-neutral-200 bg-white p-5 shadow-xl dark:border-neutral-700 dark:bg-neutral-900`}
      >
        <div className="mb-4 flex items-center justify-between gap-3">
          <h2 className="text-lg font-semibold text-neutral-900 dark:text-white">{title}</h2>
          <button type="button" aria-label="Close" className={iconBtn} onClick={onClose}>
            <X className="h-4 w-4" />
          </button>
        </div>
        {children}
      </div>
    </div>
  );
}

function ErrorBar({ text, onClose }: { text: string; onClose?: () => void }) {
  return (
    <div className="flex items-start gap-2 rounded-lg border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700 dark:border-red-900/60 dark:bg-red-950/40 dark:text-red-300">
      <AlertCircle className="mt-0.5 h-4 w-4 shrink-0" />
      <p className="flex-1 whitespace-pre-wrap">{text}</p>
      {onClose && (
        <button type="button" aria-label="Dismiss" onClick={onClose} className="shrink-0">
          <X className="h-4 w-4" />
        </button>
      )}
    </div>
  );
}

/** Screenshots for the model: pick, drop or paste; thumbnails with a remove button. */
function Shots({
  images,
  onChange,
  disabled,
}: {
  images: File[];
  onChange: (f: File[]) => void;
  disabled?: boolean;
}) {
  const urls = useMemo(() => images.map((f) => URL.createObjectURL(f)), [images]);
  useEffect(() => () => urls.forEach((u) => URL.revokeObjectURL(u)), [urls]);
  return (
    <div className="flex flex-wrap items-center gap-2">
      {images.map((f, i) => (
        <span key={`${f.name}-${i}`} className="relative">
          <img
            src={urls[i]}
            alt={f.name}
            className="h-14 w-20 rounded-md border border-neutral-200 object-cover dark:border-neutral-700"
          />
          <button
            type="button"
            aria-label={`Remove ${f.name}`}
            disabled={disabled}
            onClick={() => onChange(images.filter((_, j) => j !== i))}
            className="absolute -right-1.5 -top-1.5 rounded-full bg-neutral-900 p-0.5 text-white dark:bg-white dark:text-neutral-900"
          >
            <X className="h-3 w-3" />
          </button>
        </span>
      ))}
      <button
        type="button"
        className={btn}
        disabled={disabled}
        onClick={() =>
          void files
            .pick({ accept: "image/*", multiple: true })
            .then((picked) => picked.length && onChange([...images, ...picked]))
        }
      >
        <ImagePlus className="h-4 w-4" /> Add screenshots
      </button>
    </div>
  );
}

/** Images dropped or pasted. */
function imagesOf(list: FileList | null | undefined): File[] {
  return Array.from(list ?? []).filter((f) => f.type.startsWith("image/"));
}

/** A code window: a plain textarea with line numbers, Tab indents, Ctrl/Cmd+S saves. */
function CodeEditor({
  value,
  onChange,
  onSave,
}: {
  value: string;
  onChange: (v: string) => void;
  onSave: () => void;
}) {
  const gutter = useRef<HTMLPreElement>(null);
  const numbers = useMemo(() => {
    const n = value.split("\n").length;
    return Array.from({ length: n }, (_, i) => String(i + 1)).join("\n");
  }, [value]);
  return (
    <div className="flex min-h-0 flex-1 overflow-hidden font-mono text-[13px] leading-5">
      <pre
        ref={gutter}
        aria-hidden="true"
        className="m-0 select-none overflow-hidden border-r border-neutral-200 bg-neutral-50 px-2 py-3 text-right text-neutral-400 dark:border-neutral-800 dark:bg-neutral-950 dark:text-neutral-600"
      >
        {numbers}
      </pre>
      <textarea
        aria-label="Code"
        spellCheck={false}
        wrap="off"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onScroll={(e) => {
          if (gutter.current) gutter.current.scrollTop = e.currentTarget.scrollTop;
        }}
        onKeyDown={(e) => {
          if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
            e.preventDefault();
            onSave();
          } else if (e.key === "Tab" && !e.shiftKey) {
            e.preventDefault();
            const el = e.currentTarget;
            el.setRangeText("    ", el.selectionStart, el.selectionEnd, "end");
            onChange(el.value);
          }
        }}
        className="min-h-0 flex-1 resize-none overflow-auto whitespace-pre bg-white p-3 text-neutral-800 outline-none dark:bg-[#1e1e1e] dark:text-neutral-200"
      />
    </div>
  );
}

// ------------------------------------------------------------------ the workbench

export function Workbench({ platform }: { platform: Platform }) {
  const P = platform;
  const [home, setHome] = useState<FolderInfo | null | undefined>(undefined);
  const [settings, setSettings] = useState<Settings>(DEFAULT_SETTINGS);
  const [items, setItems] = useState<Item[] | null>(null);
  const [scanning, setScanning] = useState(false);
  const [query, setQuery] = useState("");
  const [modal, setModal] = useState<Modal | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [dragging, setDragging] = useState(false);

  // The editor.
  const [open, setOpen] = useState<string | null>(null);
  const [tree, setTree] = useState<PluginFile[]>([]);
  const [dirs, setDirs] = useState<string[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [drafts, setDrafts] = useState<Record<string, string>>({});
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const [request, setRequest] = useState("");
  const [shots, setShots] = useState<File[]>([]);
  const [history, setHistory] = useState<HistoryEntry[]>([]);
  const [undo, setUndo] = useState<Undo | null>(null);
  const [working, setWorking] = useState<{ started: number; label: string } | null>(null);
  const [lastChanged, setLastChanged] = useState<string[]>([]);
  const [model, setModel] = useState("");
  const [copied, setCopied] = useState(false);
  const abort = useRef<AbortController | null>(null);
  const [, tick] = useState(0);

  const openItem = items?.find((i) => i.folderName === open) ?? null;
  const dirty = useMemo(
    () => Object.keys(drafts).filter((p) => drafts[p] !== tree.find((f) => f.path === p)?.content),
    [drafts, tree],
  );

  // Elapsed time while the model works.
  useEffect(() => {
    if (!working) return;
    const t = setInterval(() => tick((n) => n + 1), 1000);
    return () => clearInterval(t);
  }, [working]);

  // ---- the folder and its plugins

  const scan = useCallback(async () => {
    setScanning(true);
    try {
      const info = await folder.get();
      setHome(info);
      if (!info || !info.exists) {
        setItems(info ? [] : null);
        return;
      }
      const top = await folder.list("", { depth: 1 });
      const found: Item[] = [];
      for (const d of top.filter((e) => e.kind === "dir" && !e.path.startsWith("."))) {
        const inner = await folder.list(d.path, { depth: 3 });
        const paths = inner
          .filter((e) => e.kind === "file")
          .map((e) => e.path.slice(d.path.length + 1));
        const want = P.metaFiles(paths);
        const read: PluginFile[] = [];
        for (const name of want) {
          const entry = inner.find((e) => e.path === `${d.path}/${name}`);
          if (!entry || entry.size > MAX_TEXT_BYTES) continue;
          read.push({
            path: name,
            content: await folder.readText(`${d.path}/${name}`).catch(() => null),
            size: entry.size,
          });
        }
        found.push({ folderName: d.path, meta: P.describe(d.path, read) });
      }
      found.sort((a, b) => a.meta.name.localeCompare(b.meta.name));
      setItems(found);
    } catch (e) {
      setError(message(e));
    } finally {
      setScanning(false);
    }
  }, [P]);

  useEffect(() => {
    void (async () => {
      const saved = await storage.get<Partial<Settings>>("settings").catch(() => undefined);
      if (saved) setSettings({ ...DEFAULT_SETTINGS, ...saved });
      await scan();
    })();
  }, [scan]);

  const chooseFolder = async () => {
    setError(null);
    try {
      const info = await folder.choose();
      if (info) {
        closeEditor();
        await scan();
      }
    } catch (e) {
      setError(message(e));
    }
  };

  const saveSettings = async (s: Settings) => {
    setSettings(s);
    await storage.set("settings", s);
  };

  /** Every file of a plugin: text files with their content, binary ones as `null`. */
  const loadFiles = async (folderName: string): Promise<{ list: PluginFile[]; dirs: string[] }> => {
    const entries: FolderEntry[] = await folder.list(folderName, { depth: 12 });
    const cut = folderName.length + 1;
    const list: PluginFile[] = [];
    const dirList: string[] = [];
    for (const e of entries) {
      const path = e.path.slice(cut);
      if (!path || SKIPPED.test(path)) continue;
      if (e.kind === "dir") {
        dirList.push(path);
        continue;
      }
      const content = isText(path, e.size) ? await folder.readText(e.path).catch(() => null) : null;
      list.push({ path, content, size: e.size });
    }
    list.sort((a, b) => a.path.localeCompare(b.path));
    return { list, dirs: dirList };
  };

  const zipOf = async (folderName: string, meta: PluginMeta, list: PluginFile[]) => {
    const zip = new JSZip();
    const root = P.zipRoot(meta, folderName);
    for (const f of list) {
      const name = root ? `${root}/${f.path}` : f.path;
      zip.file(
        name,
        f.content !== null ? f.content : await folder.readBytes(`${folderName}/${f.path}`),
      );
    }
    return zip.generateAsync({ type: "uint8array", compression: "DEFLATE" });
  };

  const download = async (item: Item, list?: PluginFile[]) => {
    setError(null);
    setBusy(`Packing ${item.meta.name}…`);
    try {
      const all = list ?? (await loadFiles(item.folderName)).list;
      const meta = list ? P.describe(item.folderName, all) : item.meta;
      const bytes = await zipOf(item.folderName, meta, all);
      await files.save(P.zipName(meta, item.folderName), bytes, { mediaType: "application/zip" });
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(null);
    }
  };

  // ---- the editor

  const openEditor = async (folderName: string, keepSelection = false) => {
    setError(null);
    setBusy("Opening…");
    try {
      const { list, dirs: d } = await loadFiles(folderName);
      setTree(list);
      setDirs(d);
      setOpen(folderName);
      setDrafts({});
      if (!keepSelection) {
        setRequest("");
        setShots([]);
        setLastChanged([]);
        setUndo((u) => (u && u.folderName === folderName ? u : null));
        const first =
          list.find((f) => f.content !== null && /(admin|site)\.php$/.test(f.path)) ??
          list.find((f) => f.content !== null && f.path.endsWith(".php")) ??
          list.find((f) => f.content !== null);
        setSelected(first?.path ?? null);
      }
      setHistory(
        (await storage.get<HistoryEntry[]>(`history:${folderName}`).catch(() => undefined)) ?? [],
      );
      const info = await ai.info().catch(() => ({ textModel: null }));
      setModel(info.textModel ?? "");
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(null);
    }
  };

  const reload = async (folderName: string) => {
    const { list, dirs: d } = await loadFiles(folderName);
    setTree(list);
    setDirs(d);
    return list;
  };

  function closeEditor() {
    setOpen(null);
    setTree([]);
    setDirs([]);
    setDrafts({});
    setSelected(null);
  }

  /** Leaves the editor, asking first when edits aren't saved. */
  const leave = (then: () => void) => {
    if (dirty.length > 0) setModal({ kind: "leave", then });
    else then();
  };

  const saveFiles = async (paths: string[]) => {
    if (!open) return;
    for (const p of paths) {
      const text = drafts[p];
      if (text === undefined) continue;
      await folder.write(`${open}/${p}`, text);
    }
    setTree((t) =>
      t.map((f) =>
        paths.includes(f.path) && drafts[f.path] !== undefined
          ? { ...f, content: drafts[f.path] ?? f.content }
          : f,
      ),
    );
    setDrafts((d) => {
      const next = { ...d };
      for (const p of paths) delete next[p];
      return next;
    });
  };

  const save = async (paths: string[]) => {
    setError(null);
    try {
      await saveFiles(paths);
      await scan();
    } catch (e) {
      setError(message(e));
    }
  };

  const addHistory = async (folderName: string, entry: HistoryEntry) => {
    const list = [entry, ...((await storage.get<HistoryEntry[]>(`history:${folderName}`)) ?? [])];
    const kept = list.slice(0, 30);
    await storage.set(`history:${folderName}`, kept);
    if (folderName === open) setHistory(kept);
  };

  /** Writes the model's files into a plugin folder; returns what changed (and the undo). */
  const applyAnswer = async (
    folderName: string,
    answer: AiAnswer,
    before: PluginFile[],
  ): Promise<{ changed: string[]; undo: Undo }> => {
    if (!Array.isArray(answer.files))
      throw new Error("The model's answer had no files. Try again.");
    const written = new Map<string, string>();
    const prefix = `${folderName}/`;
    for (const f of answer.files) {
      let path = typeof f?.path === "string" ? cleanPath(f.path) : "";
      // Paths are relative to the plugin's folder; a model may still put the folder first.
      if (path.startsWith(prefix)) path = path.slice(prefix.length);
      if (!path || typeof f.content !== "string" || SKIPPED.test(path)) continue;
      written.set(path, f.content);
    }
    const deleted = (Array.isArray(answer.delete) ? answer.delete : [])
      .map((p) => (typeof p === "string" ? cleanPath(p) : ""))
      .map((p) => (p.startsWith(prefix) ? p.slice(prefix.length) : p))
      .filter((p) => p && !written.has(p) && before.some((f) => f.path === p));
    if (written.size === 0 && deleted.length === 0) {
      throw new Error("The model didn't change any files. Try saying more about what to do.");
    }
    const undoList: Undo["before"] = [];
    const changed: string[] = [];
    for (const [path, content] of written) {
      const old = before.find((f) => f.path === path);
      if (old && old.content === content) continue;
      if (old && old.content === null) continue; // binary files aren't replaced by text
      undoList.push({ path, content: old ? old.content : null });
      await folder.write(`${folderName}/${path}`, content);
      changed.push(path);
    }
    for (const path of deleted) {
      const old = before.find((f) => f.path === path);
      if (!old || old.content === null) continue;
      undoList.push({ path, content: old.content });
      await folder.delete(`${folderName}/${path}`);
      changed.push(`${path} (deleted)`);
    }
    return { changed, undo: { folderName, before: undoList } };
  };

  const ask = async (prompt: string, images: File[], label: string) => {
    abort.current = new AbortController();
    setWorking({ started: Date.now(), label });
    try {
      return await ai.json<AiAnswer>({
        system: P.system(settings),
        prompt,
        images: images.length ? images : undefined,
        schema: ANSWER_SCHEMA,
        maxTokens: 32000,
        signal: abort.current.signal,
      });
    } finally {
      setWorking(null);
      abort.current = null;
    }
  };

  const change = async () => {
    if (!open || !openItem || !request.trim() || working) return;
    setError(null);
    try {
      if (dirty.length) await saveFiles(dirty);
      const current = await reload(open);
      const meta = P.describe(open, current);
      const sendable = current.map((f) =>
        f.content !== null && f.content.length > MAX_PROMPT_FILE ? { ...f, content: null } : f,
      );
      const answer = await ask(
        P.changePrompt({ meta, files: sendable, request: request.trim() }),
        shots,
        `Changing ${meta.name}`,
      );
      const { changed, undo: u } = await applyAnswer(open, answer, current);
      await addHistory(open, {
        at: Date.now(),
        request: request.trim(),
        summary: answer.summary?.trim() || "Changed the files.",
        changed,
      });
      setUndo(u.before.length ? u : null);
      setLastChanged(changed);
      setRequest("");
      setShots([]);
      await reload(open);
      if (changed[0] && !changed[0].endsWith("(deleted)")) setSelected(changed[0]);
      await scan();
    } catch (e) {
      if ((e as { code?: string })?.code !== "cancelled") setError(message(e));
    }
  };

  const undoLast = async () => {
    if (!undo || undo.folderName !== open) return;
    setError(null);
    try {
      for (const b of undo.before) {
        if (b.content === null)
          await folder.delete(`${undo.folderName}/${b.path}`).catch(() => undefined);
        else await folder.write(`${undo.folderName}/${b.path}`, b.content);
      }
      await addHistory(undo.folderName, {
        at: Date.now(),
        request: "Undo",
        summary: "Put back the files as they were before the last AI change.",
        changed: undo.before.map((b) => b.path),
      });
      setUndo(null);
      setLastChanged([]);
      setDrafts({});
      await reload(undo.folderName);
      await scan();
    } catch (e) {
      setError(message(e));
    }
  };

  // ---- new, clone, delete, import

  const create = async (name: string, slug: string, req: string, images: File[]) => {
    setError(null);
    const folderName = P.folderName(slug);
    if (items?.some((i) => i.folderName === folderName)) {
      throw new Error(`The folder ${folderName} already exists. Pick another name.`);
    }
    const answer = await ask(
      P.newPrompt({ name, slug, request: req, settings }),
      images,
      `Making ${name}`,
    );
    const { changed } = await applyAnswer(folderName, answer, []);
    await addHistory(folderName, {
      at: Date.now(),
      request: req,
      summary: answer.summary?.trim() || `Made ${name}.`,
      changed,
    });
    await scan();
    await openEditor(folderName);
    setLastChanged(changed);
  };

  const cloneItem = async (item: Item, name: string, slug: string) => {
    const folderName = P.folderName(slug);
    if (items?.some((i) => i.folderName === folderName)) {
      throw new Error(`The folder ${folderName} already exists. Pick another name.`);
    }
    setBusy(`Cloning ${item.meta.name}…`);
    try {
      const { list } = await loadFiles(item.folderName);
      const rename = renamer(P.slugForms(item.meta.slug), P.slugForms(slug), item.meta.name, name);
      for (const f of list) {
        const path = rename(f.path);
        if (f.content !== null) await folder.write(`${folderName}/${path}`, rename(f.content));
        else
          await folder.write(
            `${folderName}/${path}`,
            await folder.readBytes(`${item.folderName}/${f.path}`),
          );
      }
      await addHistory(folderName, {
        at: Date.now(),
        request: `Clone of ${item.meta.name}`,
        summary: `Copied ${item.folderName} and renamed ${item.meta.slug} to ${slug}.`,
        changed: [],
      });
      await scan();
      await openEditor(folderName);
    } finally {
      setBusy(null);
    }
  };

  const deleteItem = async (item: Item) => {
    setError(null);
    try {
      await folder.delete(item.folderName);
      await storage.delete(`history:${item.folderName}`);
      if (open === item.folderName) closeEditor();
      await scan();
    } catch (e) {
      setError(message(e));
    }
  };

  const writeImport = async (folderName: string, list: ImportFile[], replace: boolean) => {
    setBusy("Importing…");
    try {
      if (replace) await folder.delete(folderName).catch(() => undefined);
      for (const f of list) await folder.write(`${folderName}/${f.path}`, f.data);
      await addHistory(folderName, {
        at: Date.now(),
        request: "Import",
        summary: `Imported from a ZIP file (${list.length} files).`,
        changed: [],
      });
      await scan();
      await openEditor(folderName);
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(null);
    }
  };

  const importZip = async (zipFile: File | undefined) => {
    if (!zipFile) return;
    setError(null);
    if (!/\.zip$/i.test(zipFile.name)) {
      setError("Choose a .zip file.");
      return;
    }
    try {
      const zip = await JSZip.loadAsync(zipFile);
      let list: ImportFile[] = [];
      for (const [path, entry] of Object.entries(zip.files)) {
        const clean = cleanPath(path);
        if (entry.dir || !clean || SKIPPED.test(clean)) continue;
        list.push({ path: clean, data: await entry.async("uint8array") });
      }
      if (list.length === 0) throw new Error("The ZIP file is empty.");
      // One folder around everything: that's the plugin's folder.
      const tops = new Set(list.map((f) => (f.path.includes("/") ? f.path.split("/")[0] : "")));
      let top: string | null = null;
      if (tops.size === 1 && !tops.has("")) {
        top = [...tops][0] ?? null;
        list = list.map((f) => ({ ...f, path: f.path.slice((top ?? "").length + 1) }));
      }
      const asFiles: PluginFile[] = list.map((f) => ({
        path: f.path,
        content: isText(f.path, f.data.length) ? bytesToText(f.data) : null,
        size: f.data.length,
      }));
      const folderName = P.importFolder(top, asFiles);
      if (!folderName) {
        throw new Error(`This ZIP doesn't look like an installable ${P.noun} plugin.`);
      }
      if (items?.some((i) => i.folderName === folderName)) {
        setModal({ kind: "replace", folderName, files: list });
        return;
      }
      await writeImport(folderName, list, false);
    } catch (e) {
      setError(`The ZIP couldn't be imported: ${message(e)}`);
    }
  };

  const pickZip = () =>
    void files.pick({ accept: ".zip,application/zip" }).then((f) => importZip(f[0]));

  // ---- file actions in the editor

  const fileAction = async (fn: () => Promise<void>) => {
    setError(null);
    try {
      await fn();
      if (open) await reload(open);
      await scan();
    } catch (e) {
      setError(message(e));
    }
  };

  // ------------------------------------------------------------------ render

  const header = (
    <header className="flex flex-wrap items-center gap-3 border-b border-neutral-200 px-5 py-3 dark:border-neutral-800">
      <span className="rounded-lg bg-neutral-900 p-1.5 text-white dark:bg-white dark:text-neutral-900">
        <P.Icon className="h-5 w-5" />
      </span>
      <div className="min-w-0 flex-1">
        <h1 className="truncate text-lg font-semibold text-neutral-900 dark:text-white">
          {P.title}
        </h1>
        <p className="truncate text-xs text-neutral-500 dark:text-neutral-400">{P.tagline}</p>
      </div>
      {home && (
        <span
          title={home.path}
          className="hidden max-w-[260px] items-center gap-1.5 truncate rounded-md bg-neutral-100 px-2 py-1 text-xs text-neutral-600 sm:flex dark:bg-neutral-800 dark:text-neutral-300"
        >
          <FolderOpen className="h-3.5 w-3.5 shrink-0" />
          <span className="truncate">{home.name}</span>
        </span>
      )}
      <button type="button" className={btn} onClick={() => setModal({ kind: "settings" })}>
        <SettingsIcon className="h-4 w-4" /> Settings
      </button>
    </header>
  );

  const modals = modal && (
    <ModalView
      modal={modal}
      platform={P}
      settings={settings}
      home={home ?? null}
      working={working}
      items={items ?? []}
      onClose={() => setModal(null)}
      onChooseFolder={chooseFolder}
      onSaveSettings={saveSettings}
      onCreate={create}
      onStop={() => abort.current?.abort()}
      onClone={cloneItem}
      onDelete={deleteItem}
      onReplace={(folderName, list) => void writeImport(folderName, list, true)}
      onDiscard={() => setDrafts({})}
      onSaveAll={() => saveFiles(dirty)}
      onNewFile={(path, dir) =>
        fileAction(async () => {
          if (!open) return;
          const clean = cleanPath(path);
          if (!clean) throw new Error("Give the file a name.");
          if (dir) await folder.mkdir(`${open}/${clean}`);
          else {
            await folder.write(`${open}/${clean}`, "");
            setSelected(clean);
          }
        })
      }
      onRenameFile={(from, to) =>
        fileAction(async () => {
          if (!open) return;
          const clean = cleanPath(to);
          if (!clean) throw new Error("Give the file a name.");
          await folder.rename(`${open}/${from}`, `${open}/${clean}`);
          setDrafts((d) => {
            const next = { ...d };
            delete next[from];
            return next;
          });
          if (selected === from) setSelected(clean);
        })
      }
      onDeleteFile={(path) =>
        fileAction(async () => {
          if (!open) return;
          await folder.delete(`${open}/${path}`);
          setDrafts((d) => {
            const next = { ...d };
            delete next[path];
            return next;
          });
          if (selected && (selected === path || selected.startsWith(`${path}/`))) setSelected(null);
        })
      }
    />
  );

  // No folder yet.
  if (home === undefined) {
    return (
      <div className="flex h-screen items-center justify-center text-neutral-500">
        <Loader2 className="mr-2 h-5 w-5 animate-spin" /> Loading…
      </div>
    );
  }
  if (home === null || !home.exists) {
    return (
      <div className="flex min-h-screen flex-col">
        {header}
        <div className="mx-auto mt-16 max-w-lg px-6 text-center">
          <FolderOpen className="mx-auto mb-4 h-12 w-12 text-neutral-400" />
          <h2 className="mb-2 text-xl font-semibold text-neutral-900 dark:text-white">
            {home ? `The folder ${home.name} is gone` : `Choose your ${P.nounPlural} folder`}
          </h2>
          <p className="mb-6 text-sm text-neutral-600 dark:text-neutral-400">
            Every {P.noun} you make or import gets a folder of its own inside it. The app reads and
            writes only there.
          </p>
          <button type="button" className={primary} onClick={() => void chooseFolder()}>
            <FolderOpen className="h-4 w-4" /> Choose folder
          </button>
          {error && (
            <div className="mt-4 text-left">
              <ErrorBar text={error} onClose={() => setError(null)} />
            </div>
          )}
        </div>
        {modals}
      </div>
    );
  }

  // ---- the browser
  if (!open || !openItem) {
    const q = query.trim().toLowerCase();
    const shown = (items ?? []).filter(
      (i) =>
        !q ||
        i.meta.name.toLowerCase().includes(q) ||
        i.folderName.toLowerCase().includes(q) ||
        i.meta.description.toLowerCase().includes(q),
    );
    return (
      <div
        className="flex min-h-screen flex-col"
        onDragOver={(e) => {
          e.preventDefault();
          setDragging(true);
        }}
        onDragLeave={() => setDragging(false)}
        onDrop={(e) => {
          e.preventDefault();
          setDragging(false);
          void importZip(e.dataTransfer.files[0]);
        }}
      >
        {header}
        <div className="mx-auto w-full max-w-6xl flex-1 p-5">
          <div className="mb-4 flex flex-wrap items-center gap-2">
            <h2 className="mr-auto text-base font-semibold capitalize text-neutral-900 dark:text-white">
              {P.nounPlural}
              {items && (
                <span className="ml-2 text-sm font-normal text-neutral-500">{items.length}</span>
              )}
            </h2>
            <label className="relative">
              <Search className="pointer-events-none absolute left-2.5 top-2.5 h-4 w-4 text-neutral-400" />
              <input
                aria-label={`Search ${P.nounPlural}`}
                className={`${input} w-56 py-1.5 pl-8`}
                placeholder="Search…"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
              />
            </label>
            <button
              type="button"
              className={iconBtn}
              title="Refresh"
              aria-label="Refresh"
              onClick={() => void scan()}
            >
              <RefreshCw className={`h-4 w-4 ${scanning ? "animate-spin" : ""}`} />
            </button>
            <button type="button" className={btn} onClick={pickZip}>
              <Upload className="h-4 w-4" /> Import ZIP
            </button>
            <button type="button" className={primary} onClick={() => setModal({ kind: "new" })}>
              <Plus className="h-4 w-4" /> New {P.noun}
            </button>
          </div>
          {error && (
            <div className="mb-4">
              <ErrorBar text={error} onClose={() => setError(null)} />
            </div>
          )}
          {busy && (
            <p className="mb-4 flex items-center gap-2 text-sm text-neutral-500">
              <Loader2 className="h-4 w-4 animate-spin" /> {busy}
            </p>
          )}
          {dragging && (
            <div className="mb-4 rounded-xl border-2 border-dashed border-emerald-500 bg-emerald-50 p-6 text-center text-sm text-emerald-700 dark:bg-emerald-950/30 dark:text-emerald-300">
              Drop a ZIP file to import it
            </div>
          )}
          {items === null ? (
            <p className="text-sm text-neutral-500">Reading the folder…</p>
          ) : items.length === 0 ? (
            <div className="rounded-2xl border border-dashed border-neutral-300 p-10 text-center dark:border-neutral-700">
              <P.Icon className="mx-auto mb-3 h-10 w-10 text-neutral-400" />
              <p className="mb-4 text-sm text-neutral-600 dark:text-neutral-400">
                No {P.nounPlural} in {home.name} yet. Make one with AI, or import an installable ZIP
                (you can also drop it here).
              </p>
              <div className="flex justify-center gap-2">
                <button type="button" className={btn} onClick={pickZip}>
                  <Upload className="h-4 w-4" /> Import ZIP
                </button>
                <button type="button" className={primary} onClick={() => setModal({ kind: "new" })}>
                  <Plus className="h-4 w-4" /> New {P.noun}
                </button>
              </div>
            </div>
          ) : (
            <ul className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">
              {shown.map((item) => (
                <li
                  key={item.folderName}
                  className="flex flex-col rounded-xl border border-neutral-200 bg-white p-4 shadow-sm dark:border-neutral-800 dark:bg-neutral-900"
                >
                  <div className="mb-1 flex items-start gap-2">
                    <h3 className="min-w-0 flex-1 truncate font-semibold text-neutral-900 dark:text-white">
                      {item.meta.name}
                    </h3>
                    {item.meta.version && (
                      <span className="shrink-0 rounded bg-neutral-100 px-1.5 py-0.5 font-mono text-xs text-neutral-600 dark:bg-neutral-800 dark:text-neutral-300">
                        v{item.meta.version}
                      </span>
                    )}
                  </div>
                  <p className="mb-1 truncate font-mono text-xs text-neutral-500">
                    {item.folderName}
                  </p>
                  <p className="mb-3 line-clamp-2 min-h-[2.5rem] text-sm text-neutral-600 dark:text-neutral-400">
                    {item.meta.description ||
                      (item.meta.recognized ? "" : `Not a ${P.noun} plugin I recognize.`)}
                  </p>
                  <div className="mt-auto flex flex-wrap gap-1.5">
                    <button
                      type="button"
                      className={primary}
                      aria-label={`Edit ${item.meta.name}`}
                      onClick={() => void openEditor(item.folderName)}
                    >
                      <Edit3 className="h-4 w-4" /> Edit
                    </button>
                    <button
                      type="button"
                      className={btn}
                      aria-label={`Clone ${item.meta.name}`}
                      onClick={() => setModal({ kind: "clone", item })}
                    >
                      <Copy className="h-4 w-4" /> Clone
                    </button>
                    <button
                      type="button"
                      className={btn}
                      aria-label={`Download ${item.meta.name}`}
                      title="Download installable zip"
                      onClick={() => void download(item)}
                    >
                      <Download className="h-4 w-4" /> ZIP
                    </button>
                    <button
                      type="button"
                      className={`${btn} ml-auto text-red-600 dark:text-red-400`}
                      aria-label={`Delete ${item.meta.name}`}
                      onClick={() => setModal({ kind: "delete", item })}
                    >
                      <Trash2 className="h-4 w-4" />
                    </button>
                  </div>
                </li>
              ))}
            </ul>
          )}
        </div>
        {modals}
      </div>
    );
  }

  // ---- the editor
  const meta = P.describe(open, tree);
  const problems = P.check(meta, tree);
  const current = tree.find((f) => f.path === selected) ?? null;
  const text = current ? (drafts[current.path] ?? current.content) : null;
  const root = buildTree(
    tree.map((f) => f.path),
    dirs,
  );
  const selectedIsDir = selected !== null && dirs.includes(selected);

  const renderNode = (node: TreeNode, depth: number): ReactNode => {
    const kids = [...node.children.values()].sort((a, b) =>
      a.dir === b.dir ? a.name.localeCompare(b.name) : a.dir ? -1 : 1,
    );
    if (!node.path) return kids.map((k) => renderNode(k, depth));
    const pad = { paddingLeft: `${depth * 12 + 6}px` };
    const active = selected === node.path;
    const row = `flex w-full items-center gap-1.5 rounded-md px-1.5 py-1 text-left text-sm ${
      active
        ? "bg-neutral-200 font-medium text-neutral-900 dark:bg-neutral-800 dark:text-white"
        : "text-neutral-700 hover:bg-neutral-100 dark:text-neutral-300 dark:hover:bg-neutral-800/60"
    }`;
    if (!node.dir) {
      const changedNow = lastChanged.includes(node.path);
      return (
        <button
          type="button"
          key={node.path}
          title={node.path}
          className={row}
          style={pad}
          onClick={() => setSelected(node.path)}
        >
          <FileIcon className="h-4 w-4 shrink-0 opacity-60" />
          <span className="truncate">{node.name}</span>
          {dirty.includes(node.path) && (
            <span
              aria-label="unsaved"
              className="ml-auto h-2 w-2 shrink-0 rounded-full bg-amber-500"
            />
          )}
          {changedNow && !dirty.includes(node.path) && (
            <span
              aria-label="changed by AI"
              className="ml-auto h-2 w-2 shrink-0 rounded-full bg-emerald-500"
            />
          )}
        </button>
      );
    }
    const isOpen = !collapsed.has(node.path);
    return (
      <div key={node.path}>
        <button
          type="button"
          className={row}
          style={pad}
          onClick={() => {
            setSelected(node.path);
            const next = new Set(collapsed);
            if (next.has(node.path)) next.delete(node.path);
            else next.add(node.path);
            setCollapsed(next);
          }}
        >
          {isOpen ? (
            <ChevronDown className="h-3.5 w-3.5 shrink-0 opacity-50" />
          ) : (
            <ChevronRight className="h-3.5 w-3.5 shrink-0 opacity-50" />
          )}
          <FolderIcon className="h-4 w-4 shrink-0 text-blue-500 opacity-80" />
          <span className="truncate">{node.name}</span>
        </button>
        {isOpen && kids.map((k) => renderNode(k, depth + 1))}
      </div>
    );
  };

  const seconds = working ? Math.round((Date.now() - working.started) / 1000) : 0;

  return (
    <div className="flex h-screen flex-col">
      <header className="flex flex-wrap items-center gap-2 border-b border-neutral-200 px-4 py-2.5 dark:border-neutral-800">
        <button
          type="button"
          className={btn}
          aria-label={`Back to ${P.nounPlural}`}
          onClick={() =>
            leave(() => {
              closeEditor();
              void scan();
            })
          }
        >
          <ArrowLeft className="h-4 w-4" /> <span className="capitalize">{P.nounPlural}</span>
        </button>
        <div className="min-w-0 flex-1 px-1">
          <h1 className="truncate font-semibold text-neutral-900 dark:text-white">
            {meta.name}
            {meta.version && (
              <span className="ml-2 font-mono text-xs font-normal text-neutral-500">
                v{meta.version}
              </span>
            )}
          </h1>
          <p className="truncate font-mono text-xs text-neutral-500">{open}</p>
        </div>
        <button
          type="button"
          className={btn}
          onClick={() => leave(() => setModal({ kind: "clone", item: openItem }))}
        >
          <Copy className="h-4 w-4" /> Clone
        </button>
        <button
          type="button"
          className={`${btn} text-red-600 dark:text-red-400`}
          onClick={() => setModal({ kind: "delete", item: openItem })}
        >
          <Trash2 className="h-4 w-4" /> Delete
        </button>
        <button
          type="button"
          className="inline-flex items-center gap-1.5 rounded-lg bg-emerald-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-emerald-700 disabled:opacity-50"
          disabled={busy !== null}
          onClick={() =>
            void (async () => {
              if (dirty.length) await saveFiles(dirty);
              const list = await reload(open);
              await download({ folderName: open, meta: P.describe(open, list) }, list);
            })()
          }
        >
          <Download className="h-4 w-4" /> Download installable zip
        </button>
      </header>

      {(error || problems.length > 0) && (
        <div className="space-y-2 border-b border-neutral-200 px-4 py-2 dark:border-neutral-800">
          {error && <ErrorBar text={error} onClose={() => setError(null)} />}
          {problems.length > 0 && (
            <details className="rounded-lg border border-amber-200 bg-amber-50 px-3 py-2 text-sm text-amber-800 dark:border-amber-900/60 dark:bg-amber-950/30 dark:text-amber-300">
              <summary className="flex cursor-pointer items-center gap-2">
                <AlertTriangle className="h-4 w-4" />
                {problems.length === 1
                  ? "1 thing to check"
                  : `${problems.length} things to check`}{" "}
                before installing in {P.installsIn}
              </summary>
              <ul className="mt-1 list-disc pl-8">
                {problems.map((p) => (
                  <li key={p}>{p}</li>
                ))}
              </ul>
            </details>
          )}
        </div>
      )}

      <div className="flex min-h-0 flex-1">
        {/* Files */}
        <nav
          aria-label="Files"
          className="flex w-60 shrink-0 flex-col border-r border-neutral-200 bg-neutral-50/60 dark:border-neutral-800 dark:bg-neutral-900/40"
        >
          <div className="flex items-center gap-0.5 border-b border-neutral-200 px-2 py-1.5 dark:border-neutral-800">
            <span className="mr-auto text-xs font-semibold uppercase tracking-wide text-neutral-500">
              Files
            </span>
            <button
              type="button"
              className={iconBtn}
              title="New file"
              aria-label="New file"
              onClick={() => setModal({ kind: "newFile", dir: false })}
            >
              <FilePlus className="h-4 w-4" />
            </button>
            <button
              type="button"
              className={iconBtn}
              title="New folder"
              aria-label="New folder"
              onClick={() => setModal({ kind: "newFile", dir: true })}
            >
              <FolderPlus className="h-4 w-4" />
            </button>
            <button
              type="button"
              className={iconBtn}
              title="Rename"
              aria-label="Rename"
              disabled={!selected}
              onClick={() => selected && setModal({ kind: "renameFile", path: selected })}
            >
              <Edit3 className="h-4 w-4" />
            </button>
            <button
              type="button"
              className={iconBtn}
              title="Delete"
              aria-label="Delete file"
              disabled={!selected}
              onClick={() => selected && setModal({ kind: "deleteFile", path: selected })}
            >
              <Trash2 className="h-4 w-4" />
            </button>
          </div>
          <div className="min-h-0 flex-1 overflow-y-auto p-1.5">{renderNode(root, 0)}</div>
        </nav>

        {/* Code */}
        <section className="flex min-w-0 flex-1 flex-col">
          {current && text !== null ? (
            <>
              <div className="flex items-center gap-2 border-b border-neutral-200 bg-neutral-50 px-3 py-1.5 font-mono text-xs text-neutral-600 dark:border-neutral-800 dark:bg-[#252526] dark:text-neutral-400">
                <span className="truncate">{current.path}</span>
                {dirty.includes(current.path) && <span className="text-amber-600">● unsaved</span>}
                <span className="ml-auto flex items-center gap-1">
                  <button
                    type="button"
                    className={iconBtn}
                    title="Copy file"
                    aria-label="Copy file"
                    onClick={() =>
                      void app.copy(text).then(() => {
                        setCopied(true);
                        setTimeout(() => setCopied(false), 1500);
                      })
                    }
                  >
                    {copied ? (
                      <Check className="h-4 w-4 text-emerald-500" />
                    ) : (
                      <Copy className="h-4 w-4" />
                    )}
                  </button>
                  <button
                    type="button"
                    className={btn}
                    disabled={!dirty.includes(current.path)}
                    onClick={() =>
                      setDrafts((d) => {
                        const next = { ...d };
                        delete next[current.path];
                        return next;
                      })
                    }
                  >
                    Revert
                  </button>
                  <button
                    type="button"
                    className={primary}
                    disabled={!dirty.includes(current.path)}
                    onClick={() => void save([current.path])}
                  >
                    <Save className="h-4 w-4" /> Save
                  </button>
                </span>
              </div>
              <CodeEditor
                key={current.path}
                value={text}
                onChange={(v) => setDrafts((d) => ({ ...d, [current.path]: v }))}
                onSave={() => void save([current.path])}
              />
            </>
          ) : current ? (
            <div className="flex flex-1 items-center justify-center text-sm text-neutral-500">
              {current.path} is a binary file ({formatSize(current.size)}). It goes into the ZIP as
              it is.
            </div>
          ) : (
            <div className="flex flex-1 items-center justify-center text-sm text-neutral-500">
              {selectedIsDir ? "A folder. Pick a file to edit it." : "Pick a file to edit it."}
            </div>
          )}
        </section>

        {/* AI */}
        <aside
          className="flex w-80 shrink-0 flex-col border-l border-neutral-200 dark:border-neutral-800"
          onDragOver={(e) => e.preventDefault()}
          onDrop={(e) => {
            e.preventDefault();
            e.stopPropagation();
            const imgs = imagesOf(e.dataTransfer.files);
            if (imgs.length) setShots((s) => [...s, ...imgs]);
          }}
        >
          <div className="flex items-center gap-2 border-b border-neutral-200 px-3 py-2 dark:border-neutral-800">
            <Sparkles className="h-4 w-4 text-neutral-500" />
            <span className="text-sm font-semibold text-neutral-900 dark:text-white">
              Change with AI
            </span>
            {undo && undo.folderName === open && (
              <button
                type="button"
                className={`${btn} ml-auto py-1`}
                disabled={working !== null}
                onClick={() => void undoLast()}
              >
                <Undo2 className="h-4 w-4" /> Undo
              </button>
            )}
          </div>
          <ol aria-label="History" className="min-h-0 flex-1 space-y-3 overflow-y-auto p-3">
            {history.length === 0 && (
              <li className="text-sm text-neutral-500">
                Ask for a change, e.g. “add a style tab with the title color and typography”.
                Screenshots help: drop or paste them.
              </li>
            )}
            {[...history].reverse().map((h) => (
              <li key={`${h.at}-${h.request}`} className="space-y-1 text-sm">
                <p className="rounded-lg bg-neutral-100 px-2.5 py-1.5 text-neutral-800 dark:bg-neutral-800 dark:text-neutral-200">
                  {h.request}
                </p>
                <p className="px-1 text-neutral-600 dark:text-neutral-400">{h.summary}</p>
                {h.changed.length > 0 && (
                  <p className="px-1 font-mono text-[11px] text-neutral-500">
                    {h.changed.join(", ")}
                  </p>
                )}
                <p className="px-1 text-[11px] text-neutral-400">{ago(h.at)}</p>
              </li>
            ))}
          </ol>
          <div className="space-y-2 border-t border-neutral-200 p-3 dark:border-neutral-800">
            <textarea
              aria-label="What to change"
              className={`${input} h-28 resize-y`}
              placeholder={`What should change in this ${P.noun}?`}
              value={request}
              disabled={working !== null}
              onChange={(e) => setRequest(e.target.value)}
              onPaste={(e) => {
                const imgs = imagesOf(e.clipboardData?.files);
                if (imgs.length) {
                  e.preventDefault();
                  setShots((s) => [...s, ...imgs]);
                }
              }}
              onKeyDown={(e) => {
                if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
                  e.preventDefault();
                  void change();
                }
              }}
            />
            <Shots images={shots} onChange={setShots} disabled={working !== null} />
            {working ? (
              <div className="flex items-center gap-2">
                <span className="flex flex-1 items-center gap-2 text-sm text-neutral-600 dark:text-neutral-400">
                  <Loader2 className="h-4 w-4 animate-spin" /> {working.label} · {seconds}s
                </span>
                <button type="button" className={btn} onClick={() => abort.current?.abort()}>
                  <Square className="h-3.5 w-3.5" /> Stop
                </button>
              </div>
            ) : (
              <button
                type="button"
                className={`${primary} w-full py-2`}
                disabled={!request.trim()}
                onClick={() => void change()}
              >
                <Sparkles className="h-4 w-4" /> Apply changes
              </button>
            )}
            {dirty.length > 0 && !working && (
              <p className="text-xs text-neutral-500">Your unsaved edits are saved first.</p>
            )}
            {model && <p className="truncate font-mono text-[11px] text-neutral-400">{model}</p>}
          </div>
        </aside>
      </div>
      {modals}
    </div>
  );
}

// ------------------------------------------------------------------ dialogs

function ModalView(props: {
  modal: Modal;
  platform: Platform;
  settings: Settings;
  home: FolderInfo | null;
  working: { started: number; label: string } | null;
  items: Item[];
  onClose: () => void;
  onChooseFolder: () => Promise<void>;
  onSaveSettings: (s: Settings) => Promise<void>;
  onCreate: (name: string, slug: string, request: string, images: File[]) => Promise<void>;
  onStop: () => void;
  onClone: (item: Item, name: string, slug: string) => Promise<void>;
  onDelete: (item: Item) => Promise<void>;
  onReplace: (folderName: string, files: ImportFile[]) => void;
  onDiscard: () => void;
  onSaveAll: () => Promise<void>;
  onNewFile: (path: string, dir: boolean) => Promise<void>;
  onRenameFile: (from: string, to: string) => Promise<void>;
  onDeleteFile: (path: string) => Promise<void>;
}) {
  const { modal, onClose } = props;
  switch (modal.kind) {
    case "settings":
      return <SettingsDialog {...props} />;
    case "new":
      return <NewDialog {...props} />;
    case "clone":
      return <CloneDialog {...props} item={modal.item} />;
    case "delete":
      return (
        <Dialog title={`Delete ${modal.item.meta.name}?`} onClose={onClose}>
          <p className="mb-4 text-sm text-neutral-600 dark:text-neutral-400">
            This deletes the folder <span className="font-mono">{modal.item.folderName}</span> and
            every file in it, for good. Download its ZIP first if you might need it again.
          </p>
          <div className="flex justify-end gap-2">
            <button type="button" className={btn} onClick={onClose}>
              Cancel
            </button>
            <button
              type="button"
              className={danger}
              onClick={() => {
                onClose();
                void props.onDelete(modal.item);
              }}
            >
              <Trash2 className="h-4 w-4" /> Delete
            </button>
          </div>
        </Dialog>
      );
    case "replace":
      return (
        <Dialog title="Replace it?" onClose={onClose}>
          <p className="mb-4 text-sm text-neutral-600 dark:text-neutral-400">
            <span className="font-mono">{modal.folderName}</span> is already in your folder. Replace
            it with the ZIP's files? Files that aren't in the ZIP are removed.
          </p>
          <div className="flex justify-end gap-2">
            <button type="button" className={btn} onClick={onClose}>
              Cancel
            </button>
            <button
              type="button"
              className={danger}
              onClick={() => {
                onClose();
                props.onReplace(modal.folderName, modal.files);
              }}
            >
              Replace
            </button>
          </div>
        </Dialog>
      );
    case "leave":
      return (
        <Dialog title="Unsaved edits" onClose={onClose}>
          <p className="mb-4 text-sm text-neutral-600 dark:text-neutral-400">
            Some files have edits that aren't saved.
          </p>
          <div className="flex justify-end gap-2">
            <button type="button" className={btn} onClick={onClose}>
              Stay
            </button>
            <button
              type="button"
              className={btn}
              onClick={() => {
                props.onDiscard();
                onClose();
                modal.then();
              }}
            >
              Discard
            </button>
            <button
              type="button"
              className={primary}
              onClick={() =>
                void props.onSaveAll().then(() => {
                  onClose();
                  modal.then();
                })
              }
            >
              <Save className="h-4 w-4" /> Save all
            </button>
          </div>
        </Dialog>
      );
    case "newFile":
    case "renameFile":
      return (
        <NameDialog
          title={modal.kind === "renameFile" ? "Rename" : modal.dir ? "New folder" : "New file"}
          label="Path inside the plugin"
          initial={modal.kind === "renameFile" ? modal.path : ""}
          placeholder={
            modal.kind === "newFile" && modal.dir ? "assets/images" : "assets/css/style.css"
          }
          onClose={onClose}
          onDone={async (v) => {
            onClose();
            if (modal.kind === "renameFile") await props.onRenameFile(modal.path, v);
            else await props.onNewFile(v, modal.dir);
          }}
        />
      );
    case "deleteFile":
      return (
        <Dialog title={`Delete ${modal.path}?`} onClose={onClose}>
          <p className="mb-4 text-sm text-neutral-600 dark:text-neutral-400">
            This deletes it from the plugin's folder for good.
          </p>
          <div className="flex justify-end gap-2">
            <button type="button" className={btn} onClick={onClose}>
              Cancel
            </button>
            <button
              type="button"
              className={danger}
              onClick={() => {
                onClose();
                void props.onDeleteFile(modal.path);
              }}
            >
              <Trash2 className="h-4 w-4" /> Delete
            </button>
          </div>
        </Dialog>
      );
  }
}

function NameDialog({
  title,
  label,
  initial,
  placeholder,
  onClose,
  onDone,
}: {
  title: string;
  label: string;
  initial: string;
  placeholder: string;
  onClose: () => void;
  onDone: (v: string) => Promise<void>;
}) {
  const [value, setValue] = useState(initial);
  return (
    <Dialog title={title} onClose={onClose}>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          if (value.trim()) void onDone(value.trim());
        }}
      >
        <label className="mb-4 block space-y-1.5">
          <span className="text-sm font-medium text-neutral-700 dark:text-neutral-300">
            {label}
          </span>
          <input
            className={`${input} font-mono`}
            value={value}
            placeholder={placeholder}
            onChange={(e) => setValue(e.target.value)}
          />
        </label>
        <div className="flex justify-end gap-2">
          <button type="button" className={btn} onClick={onClose}>
            Cancel
          </button>
          <button type="submit" className={primary} disabled={!value.trim()}>
            OK
          </button>
        </div>
      </form>
    </Dialog>
  );
}

function SettingsDialog({
  platform: P,
  settings,
  home,
  onClose,
  onChooseFolder,
  onSaveSettings,
}: {
  platform: Platform;
  settings: Settings;
  home: FolderInfo | null;
  onClose: () => void;
  onChooseFolder: () => Promise<void>;
  onSaveSettings: (s: Settings) => Promise<void>;
}) {
  const [s, setS] = useState(settings);
  const field = (key: keyof Settings, label: string, placeholder: string, help?: string) => (
    <label className="block space-y-1">
      <span className="text-sm font-medium text-neutral-700 dark:text-neutral-300">{label}</span>
      <input
        className={input}
        value={s[key]}
        placeholder={placeholder}
        onChange={(e) => setS({ ...s, [key]: e.target.value })}
      />
      {help && <span className="block text-xs text-neutral-500">{help}</span>}
    </label>
  );
  return (
    <Dialog title="Settings" onClose={onClose} wide>
      <div className="space-y-4">
        <div className="space-y-1">
          <span className="text-sm font-medium capitalize text-neutral-700 dark:text-neutral-300">
            {P.nounPlural} folder
          </span>
          <div className="flex items-center gap-2">
            <span
              title={home?.path}
              className="min-w-0 flex-1 truncate rounded-lg border border-neutral-200 bg-neutral-50 px-3 py-2 font-mono text-xs text-neutral-700 dark:border-neutral-700 dark:bg-neutral-800 dark:text-neutral-300"
            >
              {home?.path ?? "No folder chosen"}
            </span>
            <button type="button" className={btn} onClick={() => void onChooseFolder()}>
              <FolderOpen className="h-4 w-4" /> {home ? "Change…" : "Choose…"}
            </button>
          </div>
          <span className="block text-xs text-neutral-500">
            Each {P.noun} is a folder inside it. Changing it doesn't move anything.
          </span>
        </div>
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
          {field("author", "Author", "Your name or company")}
          {field("authorEmail", "Author e-mail", "info@example.com")}
          {field("authorUrl", "Author website", "https://example.com")}
          {field("category", P.categoryLabel, "", P.categoryHelp)}
          {P.vendorLabel && field("vendor", P.vendorLabel, "Acme", "PascalCase, e.g. C10 or Acme.")}
        </div>
        <label className="block space-y-1">
          <span className="text-sm font-medium text-neutral-700 dark:text-neutral-300">
            Always tell the AI
          </span>
          <textarea
            className={`${input} h-24 resize-y`}
            value={s.instructions}
            placeholder="e.g. Write the labels in Danish. Use 4 spaces. Keep CSS in assets/css."
            onChange={(e) => setS({ ...s, instructions: e.target.value })}
          />
        </label>
        <div className="flex justify-end gap-2">
          <button type="button" className={btn} onClick={onClose}>
            Cancel
          </button>
          <button
            type="button"
            className={primary}
            onClick={() => void onSaveSettings(s).then(onClose)}
          >
            Save
          </button>
        </div>
      </div>
    </Dialog>
  );
}

function NewDialog({
  platform: P,
  items,
  working,
  onClose,
  onCreate,
  onStop,
}: {
  platform: Platform;
  items: Item[];
  working: { started: number; label: string } | null;
  onClose: () => void;
  onCreate: (name: string, slug: string, request: string, images: File[]) => Promise<void>;
  onStop: () => void;
}) {
  const [name, setName] = useState("");
  const [slug, setSlug] = useState("");
  const [slugEdited, setSlugEdited] = useState(false);
  const [req, setReq] = useState("");
  const [images, setImages] = useState<File[]>([]);
  const [error, setError] = useState<string | null>(null);
  const effective = slugEdited ? slug : P.slugify(name);
  const taken = items.some((i) => i.folderName === P.folderName(effective));
  const problem = effective ? (taken ? "That name is taken." : P.slugProblem(effective)) : null;
  const ready = name.trim() && effective && !problem && (req.trim() || images.length > 0);

  return (
    <Dialog title={`New ${P.noun}`} onClose={() => !working && onClose()} wide>
      <form
        className="space-y-4"
        onSubmit={(e) => {
          e.preventDefault();
          if (!ready || working) return;
          setError(null);
          onCreate(name.trim(), effective, req.trim(), images)
            .then(onClose)
            .catch((err: unknown) => {
              if ((err as { code?: string })?.code !== "cancelled") setError(message(err));
            });
        }}
        onDragOver={(e) => e.preventDefault()}
        onDrop={(e) => {
          e.preventDefault();
          e.stopPropagation();
          const imgs = imagesOf(e.dataTransfer.files);
          if (imgs.length) setImages((s) => [...s, ...imgs]);
        }}
      >
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
          <label className="block space-y-1">
            <span className="text-sm font-medium text-neutral-700 dark:text-neutral-300">Name</span>
            <input
              className={input}
              value={name}
              placeholder="e.g. C10 FAQ"
              disabled={working !== null}
              onChange={(e) => setName(e.target.value)}
            />
          </label>
          <div className="space-y-1">
            <label className="block space-y-1">
              <span className="text-sm font-medium text-neutral-700 dark:text-neutral-300">
                Machine name
              </span>
              <input
                className={`${input} font-mono`}
                value={effective}
                disabled={working !== null}
                onChange={(e) => {
                  setSlugEdited(true);
                  setSlug(e.target.value);
                }}
              />
            </label>
            <span className={`block text-xs ${problem ? "text-red-600" : "text-neutral-500"}`}>
              {problem ?? (effective ? `Folder: ${P.folderName(effective)}` : " ")}
            </span>
          </div>
        </div>
        <label className="block space-y-1">
          <span className="text-sm font-medium text-neutral-700 dark:text-neutral-300">
            What should it do and look like?
          </span>
          <textarea
            className={`${input} h-36 resize-y`}
            value={req}
            disabled={working !== null}
            placeholder={P.ideas[0] ?? ""}
            onChange={(e) => setReq(e.target.value)}
            onPaste={(e) => {
              const imgs = imagesOf(e.clipboardData?.files);
              if (imgs.length) {
                e.preventDefault();
                setImages((s) => [...s, ...imgs]);
              }
            }}
          />
        </label>
        {!req && (
          <div className="flex flex-wrap gap-1.5">
            {P.ideas.map((idea) => (
              <button
                type="button"
                key={idea}
                className="rounded-full border border-neutral-200 px-2.5 py-1 text-xs text-neutral-600 hover:bg-neutral-100 dark:border-neutral-700 dark:text-neutral-300 dark:hover:bg-neutral-800"
                onClick={() => setReq(idea)}
              >
                {idea.length > 60 ? `${idea.slice(0, 57)}…` : idea}
              </button>
            ))}
          </div>
        )}
        <div className="space-y-1">
          <Shots images={images} onChange={setImages} disabled={working !== null} />
          <p className="text-xs text-neutral-500">
            Screenshots of a design are sent to the model (it needs to be able to see images).
          </p>
        </div>
        {error && <ErrorBar text={error} />}
        <div className="flex items-center justify-end gap-2">
          {working ? (
            <>
              <span className="mr-auto flex items-center gap-2 text-sm text-neutral-600 dark:text-neutral-400">
                <Loader2 className="h-4 w-4 animate-spin" /> Writing the files… (this can take a few
                minutes)
              </span>
              <button type="button" className={btn} onClick={onStop}>
                <Square className="h-3.5 w-3.5" /> Stop
              </button>
            </>
          ) : (
            <>
              <button type="button" className={btn} onClick={onClose}>
                Cancel
              </button>
              <button type="submit" className={primary} disabled={!ready}>
                <Sparkles className="h-4 w-4" /> Generate {P.noun}
              </button>
            </>
          )}
        </div>
      </form>
    </Dialog>
  );
}

function CloneDialog({
  platform: P,
  item,
  items,
  onClose,
  onClone,
}: {
  platform: Platform;
  item: Item;
  items: Item[];
  onClose: () => void;
  onClone: (item: Item, name: string, slug: string) => Promise<void>;
}) {
  const [name, setName] = useState(`${item.meta.name} copy`);
  const slug = P.slugify(name);
  const taken = items.some((i) => i.folderName === P.folderName(slug));
  const problem = !slug ? "Give it a name." : taken ? "That name is taken." : P.slugProblem(slug);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  return (
    <Dialog title={`Clone ${item.meta.name}`} onClose={onClose}>
      <form
        className="space-y-4"
        onSubmit={(e) => {
          e.preventDefault();
          if (problem || busy) return;
          setBusy(true);
          onClone(item, name.trim(), slug)
            .then(onClose)
            .catch((err: unknown) => setError(message(err)))
            .finally(() => setBusy(false));
        }}
      >
        <div className="space-y-1">
          <label className="block space-y-1">
            <span className="text-sm font-medium text-neutral-700 dark:text-neutral-300">
              Name of the copy
            </span>
            <input className={input} value={name} onChange={(e) => setName(e.target.value)} />
          </label>
          <span className={`block text-xs ${problem ? "text-red-600" : "text-neutral-500"}`}>
            {problem ?? `Folder: ${P.folderName(slug)} · ${item.meta.slug} becomes ${slug}`}
          </span>
        </div>
        <p className="text-xs text-neutral-500">
          The copy gets its own machine name, so both can be installed side by side: file names,
          class names, language keys and CSS prefixes that use “{item.meta.slug}” are renamed.
        </p>
        {error && <ErrorBar text={error} />}
        <div className="flex justify-end gap-2">
          <button type="button" className={btn} onClick={onClose}>
            Cancel
          </button>
          <button type="submit" className={primary} disabled={problem !== null || busy}>
            {busy ? <Loader2 className="h-4 w-4 animate-spin" /> : <Copy className="h-4 w-4" />}{" "}
            Clone
          </button>
        </div>
      </form>
    </Dialog>
  );
}
