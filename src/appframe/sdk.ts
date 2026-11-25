/**
 * The `oxide` module Utility Apps import (Phase 20): Oxide's models and tools, reached through
 * the main window. Every method returns a promise; failures reject with an {@link OxideError}.
 *
 * Calls that use models, the web, connectors or the computer need a capability the app's
 * manifest declares (`oxide-app.json` → `capabilities`). The first such call asks the user
 * (“Allow for this app” / “Deny”); after that it just runs. Files and storage never ask;
 * the app's own folder (`folder`) is allowed when the user picks it.
 *
 * ```tsx
 * import { ai, web } from "oxide";
 * const page = await web.fetch("https://example.com");
 * const { text } = await ai.text({ prompt: `Summarize:\n${page.content}` });
 * ```
 */
import type { SdkErrorCode } from "./protocol";

// ---------------------------------------------------------------- transport

type Post = (message: unknown) => void;

interface Pending {
  resolve: (v: unknown) => void;
  reject: (e: unknown) => void;
  onEvent?: (e: unknown) => void;
}

let post: Post = () => {
  throw new Error("The oxide SDK runs only inside Oxide's app frame.");
};
const pending = new Map<string, Pending>();
let counter = 0;
const appInfo = { id: "", name: "", theme: "light" as "light" | "dark" };

/** An SDK call failed. `code` says why: the user denied it, the manifest doesn't declare the
 * capability, it was cancelled, or it failed (the message says why). */
export class OxideError extends Error {
  code: SdkErrorCode;
  constructor(code: SdkErrorCode, message: string) {
    super(message);
    this.name = "OxideError";
    this.code = code;
  }
}

/** Wires the SDK to the frame (called by the frame before the app runs). */
export function connect(p: Post, info: { id: string; name: string; theme: "light" | "dark" }) {
  post = p;
  Object.assign(appInfo, info);
}

/** The frame got an answer or an event for a call. */
export function deliver(
  msg:
    | { type: "result"; id: string; ok: true; value: unknown }
    | { type: "result"; id: string; ok: false; error: { code: SdkErrorCode; message: string } }
    | { type: "event"; id: string; event: unknown },
) {
  const p = pending.get(msg.id);
  if (!p) return;
  if (msg.type === "event") {
    p.onEvent?.(msg.event);
    return;
  }
  pending.delete(msg.id);
  if (msg.ok) p.resolve(msg.value);
  else p.reject(new OxideError(msg.error.code, msg.error.message));
}

export function setTheme(theme: "light" | "dark") {
  appInfo.theme = theme;
}

function call<T>(
  method: string,
  params: unknown,
  opts: { onEvent?: (e: unknown) => void; signal?: AbortSignal } = {},
): Promise<T> {
  const id = `c${++counter}-${Math.random().toString(36).slice(2, 8)}`;
  return new Promise<T>((resolve, reject) => {
    if (opts.signal?.aborted) {
      reject(new OxideError("cancelled", "Stopped."));
      return;
    }
    pending.set(id, { resolve: resolve as (v: unknown) => void, reject, onEvent: opts.onEvent });
    opts.signal?.addEventListener(
      "abort",
      () => post({ source: "oxide-app", type: "cancel", id }),
      {
        once: true,
      },
    );
    post({ source: "oxide-app", type: "call", id, method, params });
  });
}

// ---------------------------------------------------------------- helpers

/** A file sent to a model or a generation endpoint: a Blob or File, or base64 data. */
export type FileInput = Blob | File | { mediaType: string; data: string; name?: string };

/** A Blob's bytes as base64 (no `data:` prefix). */
export async function toBase64(blob: Blob): Promise<string> {
  const buf = new Uint8Array(await blob.arrayBuffer());
  let s = "";
  const chunk = 0x8000;
  for (let i = 0; i < buf.length; i += chunk) {
    s += String.fromCharCode(...buf.subarray(i, i + chunk));
  }
  return btoa(s);
}

/** A Blob as a `data:` URL (e.g. for `<img src>`). */
export async function toDataUrl(blob: Blob): Promise<string> {
  return `data:${blob.type || "application/octet-stream"};base64,${await toBase64(blob)}`;
}

async function encode(f: FileInput): Promise<{ mediaType: string; data: string; name: string }> {
  if (f instanceof Blob) {
    return {
      mediaType: f.type || "application/octet-stream",
      data: await toBase64(f),
      name: f instanceof File ? f.name : "file",
    };
  }
  return { mediaType: f.mediaType, data: f.data, name: f.name ?? "file" };
}

/** A file a generation call made. `url` is an object URL for `<img>`, `<video>`, `<audio>`. */
export interface GeneratedFile {
  name: string;
  mediaType: string;
  blob: Blob;
  url: string;
}

function generated(files: Array<{ name: string; mediaType: string; blob: Blob }>): GeneratedFile[] {
  return files.map((f) => ({ ...f, url: URL.createObjectURL(f.blob) }));
}

function single(files: GeneratedFile[]): GeneratedFile {
  const first = files[0];
  if (!first) throw new OxideError("failed", "The endpoint returned no file.");
  return first;
}

// ---------------------------------------------------------------- ai

export interface Message {
  role: "user" | "assistant";
  content: string;
}

export interface TextOptions {
  /** The user's message (added after `messages`). */
  prompt?: string;
  /** Instructions for the model. */
  system?: string;
  /** Earlier turns, oldest first. */
  messages?: Message[];
  /** Images the model sees with the last message (PNG, JPEG, WebP, GIF). */
  images?: FileInput[];
  /** PDFs (and text files) the model reads with the last message. */
  documents?: FileInput[];
  maxTokens?: number;
  /** Let reasoning models think (off by default where it can be switched off). */
  thinking?: boolean;
  /** Streams the answer: each new piece and the text so far. */
  onText?: (delta: string, text: string) => void;
  signal?: AbortSignal;
}

export interface TextResult {
  text: string;
  /** The chat model that answered (`<provider>/<model>`). */
  model: string;
  /** The answer stopped at the model's output limit, so it may be incomplete. */
  truncated: boolean;
}

export interface EditOptions {
  /** The file's name or path, shown to the model (e.g. `site.php`). */
  path?: string;
  /** The file's current text. */
  content: string;
  /** What to change. */
  request: string;
  /** Instructions for the model. */
  system?: string;
  /** Other text the model should see first (e.g. the project's other files). */
  context?: string;
  /** Images the model sees with the request (screenshots). */
  images?: FileInput[];
  /** PDFs (and text files) the model reads with the request. */
  documents?: FileInput[];
  /** Output limit for an answer (default 16384 tokens). */
  maxTokens?: number;
  /** `"rewrite"` skips edit blocks and has the whole file written (most of it changes). */
  mode?: "edit" | "rewrite";
  /**
   * `false` asks the model not to think first (much faster with local reasoning models; a
   * server that refuses the switch answers with its default). Left out: the model's default.
   */
  thinking?: boolean;
  /** Hears the steps: `edits`, `retry` (the first blocks didn't apply), `rewrite`. */
  onStage?: (stage: "edits" | "retry" | "rewrite", note: string | null) => void;
  /** Streams a whole-file rewrite: each new piece and the text so far. */
  onText?: (delta: string, text: string) => void;
  signal?: AbortSignal;
}

export interface EditBlock {
  old: string;
  new: string;
  replaceAll: boolean;
}

export interface EditResult {
  /** The file's new text (the old text when nothing changed). */
  content: string;
  /** Whether `content` differs from the old text. */
  changed: boolean;
  /** The edit blocks that were applied (empty after a rewrite). */
  edits: EditBlock[];
  /** The changed lines of `content` (1-based `start`, `count` lines; 0 = lines removed). */
  ranges: Array<{ start: number; count: number }>;
  /** A short description, e.g. “2 edits applied: lines 4–6, line 20.” */
  summary: string;
  /** The file was written whole (asked for, or the edit blocks didn't apply). */
  rewritten: boolean;
  /** Why it was rewritten. */
  reason?: string;
  /** The first edit blocks didn't apply; corrected ones did. */
  retried: boolean;
  /** The chat model that answered. */
  model: string | null;
}

export interface JsonOptions extends Omit<TextOptions, "onText"> {
  /** A JSON Schema the answer must match. */
  schema?: object;
}

async function textParams(o: TextOptions) {
  return {
    prompt: o.prompt,
    system: o.system,
    messages: o.messages,
    images: o.images ? await Promise.all(o.images.map(encode)) : undefined,
    documents: o.documents ? await Promise.all(o.documents.map(encode)) : undefined,
    maxTokens: o.maxTokens,
    thinking: o.thinking,
  };
}

export interface ProgressInfo {
  percent: number | null;
  note: string;
}

function progress(cb?: (p: ProgressInfo) => void) {
  return (e: unknown) => {
    const ev = e as { type?: string; percent?: number | null; note?: string };
    if (ev.type === "progress") cb?.({ percent: ev.percent ?? null, note: ev.note ?? "" });
  };
}

export const ai = {
  /** Writes text with the app's chat model. */
  async text(options: TextOptions | string): Promise<TextResult> {
    const o = typeof options === "string" ? { prompt: options } : options;
    let all = "";
    const r = await call<{ text: string; model: string; truncated?: boolean }>(
      "text.generate",
      { ...(await textParams(o)), stream: Boolean(o.onText) },
      {
        signal: o.signal,
        onEvent: (e) => {
          const ev = e as { type?: string; text?: string };
          if (ev.type === "delta" && typeof ev.text === "string") {
            all += ev.text;
            o.onText?.(ev.text, all);
          }
        },
      },
    );
    return { text: r.text, model: r.model, truncated: r.truncated === true };
  },

  /** Asks for JSON (matching `schema` when given) and returns it parsed. */
  async json<T = unknown>(options: JsonOptions | string): Promise<T> {
    const o = typeof options === "string" ? { prompt: options } : options;
    const r = await call<{ json: T }>(
      "text.generate",
      { ...(await textParams(o)), json: o.schema ?? true },
      { signal: o.signal },
    );
    return r.json;
  },

  /**
   * Changes one text file with edit blocks: the model names exact passages and their
   * replacements, so only those parts change. Blocks that don't apply go back to the model
   * once with the error; as the last resort the whole file is written. A whole-file answer
   * that was cut off fails rather than losing the end of the file.
   */
  async edit(o: EditOptions): Promise<EditResult> {
    let all = "";
    const r = await call<EditResult>(
      "text.edit",
      {
        path: o.path,
        content: o.content,
        request: o.request,
        system: o.system,
        context: o.context,
        images: o.images ? await Promise.all(o.images.map(encode)) : undefined,
        documents: o.documents ? await Promise.all(o.documents.map(encode)) : undefined,
        maxTokens: o.maxTokens,
        mode: o.mode,
        thinking: o.thinking,
        stream: Boolean(o.onText),
      },
      {
        signal: o.signal,
        onEvent: (e) => {
          const ev = e as { type?: string; text?: string; stage?: string; note?: string | null };
          if (ev.type === "delta" && typeof ev.text === "string") {
            all += ev.text;
            o.onText?.(ev.text, all);
          } else if (
            ev.type === "stage" &&
            (ev.stage === "edits" || ev.stage === "retry" || ev.stage === "rewrite")
          ) {
            o.onStage?.(ev.stage, ev.note ?? null);
          }
        },
      },
    );
    return {
      content: r.content,
      changed: r.changed === true,
      edits: r.edits ?? [],
      ranges: r.ranges ?? [],
      summary: r.summary ?? "",
      rewritten: r.rewritten === true,
      reason: r.reason ?? undefined,
      retried: r.retried === true,
      model: r.model ?? null,
    };
  },

  /** Generates images with the app's image model. */
  async image(o: {
    prompt: string;
    n?: number;
    size?: string;
    quality?: string;
    transparent?: boolean;
    /** Images to edit or build on. */
    references?: FileInput[];
    signal?: AbortSignal;
  }): Promise<GeneratedFile[]> {
    const references = o.references ? await Promise.all(o.references.map(encode)) : undefined;
    const r = await call<{ files: Array<{ name: string; mediaType: string; blob: Blob }> }>(
      "media.image",
      {
        prompt: o.prompt,
        n: o.n,
        size: o.size,
        quality: o.quality,
        transparent: o.transparent,
        references,
      },
      { signal: o.signal },
    );
    return generated(r.files);
  },

  /** Generates a video with the app's video model (this can take many minutes). */
  async video(o: {
    prompt: string;
    seconds?: number;
    size?: string;
    /** An image to start from. */
    reference?: FileInput;
    onProgress?: (p: ProgressInfo) => void;
    signal?: AbortSignal;
  }): Promise<GeneratedFile> {
    const reference = o.reference ? await encode(o.reference) : undefined;
    const r = await call<{ files: Array<{ name: string; mediaType: string; blob: Blob }> }>(
      "media.video",
      { prompt: o.prompt, seconds: o.seconds, size: o.size, reference },
      { signal: o.signal, onEvent: progress(o.onProgress) },
    );
    return single(generated(r.files));
  },

  /** Generates music with the app's music model. */
  async music(o: {
    /** Genre and mood tags. */
    style: string;
    /** Song lyrics (`[verse]`, `[chorus]`); leave out for an instrumental. */
    lyrics?: string;
    durationSeconds?: number;
    seed?: number;
    onProgress?: (p: ProgressInfo) => void;
    signal?: AbortSignal;
  }): Promise<GeneratedFile> {
    const r = await call<{ files: Array<{ name: string; mediaType: string; blob: Blob }> }>(
      "media.music",
      { style: o.style, lyrics: o.lyrics, durationSeconds: o.durationSeconds, seed: o.seed },
      { signal: o.signal, onEvent: progress(o.onProgress) },
    );
    return single(generated(r.files));
  },

  /** The chat model text calls use now. */
  info(): Promise<{ textModel: string | null }> {
    return call("app.info", {});
  },
};

// ---------------------------------------------------------------- web

export interface SearchHit {
  title: string;
  url: string;
  snippet: string;
  age: string | null;
}

export interface Page {
  /** After redirects. */
  url: string;
  title: string | null;
  /** Markdown (default) or the page's HTML. */
  content: string;
  contentType: string;
  truncated?: boolean;
}

export interface CrawlResult {
  pages: Array<{ url: string; title: string | null; content: string; depth: number }>;
  skipped: Array<{ url: string; reason: string }>;
  /** The site had a robots.txt (which was followed). */
  robots: boolean;
}

export const web = {
  /** Searches the web with the search service set in Settings → Tools. */
  search(query: string, o: { count?: number; signal?: AbortSignal } = {}): Promise<SearchHit[]> {
    return call("web.search", { query, count: o.count }, { signal: o.signal });
  },
  /** Reads a page as Markdown, or as its HTML with `format: "html"`. */
  fetch(
    url: string,
    o: { format?: "markdown" | "html"; signal?: AbortSignal } = {},
  ): Promise<Page> {
    return call("web.fetch", { url, format: o.format }, { signal: o.signal });
  },
  /** Reads pages of one site, breadth first, following its robots.txt (at most 50 pages). */
  crawl(
    url: string,
    o: {
      maxPages?: number;
      maxDepth?: number;
      format?: "markdown" | "html";
      signal?: AbortSignal;
    } = {},
  ): Promise<CrawlResult> {
    return call(
      "web.crawl",
      { url, maxPages: o.maxPages, maxDepth: o.maxDepth, format: o.format },
      { signal: o.signal },
    );
  },
};

// ---------------------------------------------------------------- tools

export interface ToolInfo {
  name: string;
  description: string | null;
  inputSchema: unknown;
}

export interface ToolResult {
  text: string;
  images: Array<{ mediaType: string; data: string }>;
  structured: unknown;
  isError: boolean;
}

export const connectors = {
  /** The tools of the MCP servers that are running. */
  list(): Promise<Array<ToolInfo & { server: string; title: string | null; readOnly: boolean }>> {
    return call("connectors.list", {});
  },
  call(server: string, tool: string, args: Record<string, unknown> = {}): Promise<ToolResult> {
    return call("connectors.call", { server, tool, arguments: args });
  },
};

export const python = {
  /** Runs Python in Oxide's managed environment (in a working folder of the app's own). */
  run(
    code: string,
    o: { timeoutSeconds?: number; signal?: AbortSignal } = {},
  ): Promise<{ exitCode: number | null; stdout: string; stderr: string; timedOut: boolean }> {
    return call("python.run", { code, timeoutSeconds: o.timeoutSeconds }, { signal: o.signal });
  },
};

export const sandbox = {
  /** Runs a shell command in the app's Docker sandbox (`/workspace`). */
  exec(
    command: string,
    o: { cwd?: string; timeoutSeconds?: number; signal?: AbortSignal } = {},
  ): Promise<{
    exitCode: number | null;
    stdout: string;
    stderr: string;
    timedOut: boolean;
    cwd: string;
  }> {
    return call(
      "sandbox.exec",
      { command, cwd: o.cwd, timeoutSeconds: o.timeoutSeconds },
      { signal: o.signal },
    );
  },
};

export const browser = {
  /** The browser agent's tools (`navigate`, `read_page`, `browser_task`, …). */
  tools(): Promise<ToolInfo[]> {
    return call("browser.tools", {});
  },
  call(tool: string, args: Record<string, unknown> = {}): Promise<ToolResult> {
    return call("browser.call", { tool, arguments: args });
  },
};

export const chrome = {
  /** The paired Chrome's tools (`navigate`, `read_page`, `click`, …). */
  tools(): Promise<ToolInfo[]> {
    return call("chrome.tools", {});
  },
  call(tool: string, args: Record<string, unknown> = {}): Promise<ToolResult> {
    return call("chrome.call", { tool, arguments: args });
  },
};

// ---------------------------------------------------------------- files

export const files = {
  /** Lets the user pick files (`accept` like an `<input type=file>`'s). Empty when cancelled. */
  pick(o: { accept?: string; multiple?: boolean } = {}): Promise<File[]> {
    return new Promise((resolve) => {
      const input = document.createElement("input");
      input.type = "file";
      if (o.accept) input.accept = o.accept;
      input.multiple = Boolean(o.multiple);
      input.style.display = "none";
      input.addEventListener("change", () => {
        resolve(Array.from(input.files ?? []));
        input.remove();
      });
      input.addEventListener("cancel", () => {
        resolve([]);
        input.remove();
      });
      document.body.appendChild(input);
      input.click();
    });
  },
  /** Saves a file where the user picks. Resolves to whether it was saved. */
  async save(
    name: string,
    data: Blob | string | Uint8Array,
    o: { mediaType?: string } = {},
  ): Promise<boolean> {
    const blob =
      data instanceof Blob
        ? data
        : new Blob([data as BlobPart], { type: o.mediaType ?? "application/octet-stream" });
    const r = await call<{ path: string | null }>("files.save", {
      name,
      data: await toBase64(blob),
    });
    return r.path !== null;
  },
  /** A zip of the given files (made in the frame with JSZip). */
  async zip(entries: Array<{ name: string; data: Blob | string | Uint8Array }>): Promise<Blob> {
    const { default: JSZip } = await import("jszip");
    const zip = new JSZip();
    for (const e of entries) zip.file(e.name, e.data);
    return zip.generateAsync({ type: "blob" });
  },
  readText(file: Blob): Promise<string> {
    return file.text();
  },
  toBase64,
  toDataUrl,
};

// ---------------------------------------------------------------- storage

/** The app's own storage, kept by Oxide (values are JSON). */
export const storage = {
  async get<T = unknown>(key: string): Promise<T | undefined> {
    const v = await call<T | null>("storage.get", { key });
    return v === null ? undefined : v;
  },
  set(key: string, value: unknown): Promise<void> {
    return call("storage.set", { key, value });
  },
  delete(key: string): Promise<void> {
    return call("storage.delete", { key });
  },
  keys(): Promise<string[]> {
    return call("storage.keys", {});
  },
  clear(): Promise<void> {
    return call("storage.clear", {});
  },
};

// ---------------------------------------------------------------- folder

/** The folder the user picked for the app. */
export interface FolderInfo {
  /** Its full path on this computer. */
  path: string;
  /** Its name (the last part of the path). */
  name: string;
  /** Whether it still exists. */
  exists: boolean;
}

export interface FolderEntry {
  /** Relative to the app's folder, `/`-separated (`plg_x/x.xml`). */
  path: string;
  kind: "file" | "dir";
  /** Bytes (0 for folders). */
  size: number;
  /** Last change, in milliseconds since 1970 (null when unknown). */
  modified: number | null;
}

function bytesOf(b64: string): Uint8Array {
  const bin = atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

/**
 * One folder on this computer that the user picks for the app (declare `"folder"` in the
 * manifest). Paths are relative to it and `/`-separated; nothing outside it can be reached.
 * Picking the folder allows the app to use it; until then, calls other than `get` and
 * `choose` fail.
 */
export const folder = {
  /** The app's folder, or `null` when none is chosen yet. */
  get(): Promise<FolderInfo | null> {
    return call("folder.get", {});
  },
  /** Lets the user pick the app's folder (the system's folder picker). `null` when cancelled. */
  choose(): Promise<FolderInfo | null> {
    return call("folder.choose", {});
  },
  /** What a folder holds (`""`: the app's folder), `depth` levels deep (default 1, at most 12). */
  async list(path = "", o: { depth?: number } = {}): Promise<FolderEntry[]> {
    const r = await call<{ entries: FolderEntry[] }>("folder.list", { path, depth: o.depth });
    return r.entries;
  },
  /** A text file's content (UTF-8). */
  async readText(path: string): Promise<string> {
    const r = await call<{ content: string }>("folder.read", { path });
    return r.content;
  },
  /** A file's bytes. */
  async readBytes(path: string): Promise<Uint8Array> {
    const r = await call<{ content: string }>("folder.read", { path, as: "base64" });
    return bytesOf(r.content);
  },
  /** Writes a file (making its folders), replacing what was there. At most 20 MB. */
  async write(path: string, data: string | Blob | Uint8Array): Promise<void> {
    if (typeof data === "string") return call("folder.write", { path, text: data });
    const blob = data instanceof Blob ? data : new Blob([data as BlobPart]);
    return call("folder.write", { path, base64: await toBase64(blob) });
  },
  /** Makes a folder (and its parents). */
  mkdir(path: string): Promise<void> {
    return call("folder.mkdir", { path });
  },
  /** Deletes a file, or a folder with everything in it, for good. */
  delete(path: string): Promise<void> {
    return call("folder.delete", { path });
  },
  /** Renames or moves a file or folder (`to` must not exist yet). */
  rename(from: string, to: string): Promise<void> {
    return call("folder.rename", { from, to });
  },
};

// ---------------------------------------------------------------- app

export const app = {
  get id(): string {
    return appInfo.id;
  },
  get name(): string {
    return appInfo.name;
  },
  /** Oxide's theme; the frame's `<html>` has the class `dark` in the dark theme. */
  get theme(): "light" | "dark" {
    return appInfo.theme;
  },
  /** Copies text to the clipboard. */
  copy(text: string): Promise<void> {
    return call("app.copy", { text });
  },
};
