// Types of the `oxide` module that Utility Apps import (generated from sdk.ts by
// scripts/sdk-dts.mjs; run `npm run sdk-types` after changing the SDK; don't edit by hand).

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
export type SdkErrorCode =
  "needsPermission" | "undeclared" | "denied" | "cancelled" | "unknown" | "failed";
/** An SDK call failed. `code` says why: the user denied it, the manifest doesn't declare the
 * capability, it was cancelled, or it failed (the message says why). */
export declare class OxideError extends Error {
  code: SdkErrorCode;
  constructor(code: SdkErrorCode, message: string);
}
/** A file sent to a model or a generation endpoint: a Blob or File, or base64 data. */
export type FileInput =
  | Blob
  | File
  | {
      mediaType: string;
      data: string;
      name?: string;
    };
/** A Blob's bytes as base64 (no `data:` prefix). */
export declare function toBase64(blob: Blob): Promise<string>;
/** A Blob as a `data:` URL (e.g. for `<img src>`). */
export declare function toDataUrl(blob: Blob): Promise<string>;
/** A file a generation call made. `url` is an object URL for `<img>`, `<video>`, `<audio>`. */
export interface GeneratedFile {
  name: string;
  mediaType: string;
  blob: Blob;
  url: string;
}
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
  ranges: Array<{
    start: number;
    count: number;
  }>;
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
export interface ProgressInfo {
  percent: number | null;
  note: string;
}
export declare const ai: {
  /** Writes text with the app's chat model. */
  text(options: TextOptions | string): Promise<TextResult>;
  /** Asks for JSON (matching `schema` when given) and returns it parsed. */
  json<T = unknown>(options: JsonOptions | string): Promise<T>;
  /**
   * Changes one text file with edit blocks: the model names exact passages and their
   * replacements, so only those parts change. Blocks that don't apply go back to the model
   * once with the error; as the last resort the whole file is written. A whole-file answer
   * that was cut off fails rather than losing the end of the file.
   */
  edit(o: EditOptions): Promise<EditResult>;
  /** Generates images with the app's image model. */
  image(o: {
    prompt: string;
    n?: number;
    size?: string;
    quality?: string;
    transparent?: boolean;
    /** Images to edit or build on. */
    references?: FileInput[];
    signal?: AbortSignal;
  }): Promise<GeneratedFile[]>;
  /** Generates a video with the app's video model (this can take many minutes). */
  video(o: {
    prompt: string;
    seconds?: number;
    size?: string;
    /** An image to start from. */
    reference?: FileInput;
    onProgress?: (p: ProgressInfo) => void;
    signal?: AbortSignal;
  }): Promise<GeneratedFile>;
  /** Generates music with the app's music model. */
  music(o: {
    /** Genre and mood tags. */
    style: string;
    /** Song lyrics (`[verse]`, `[chorus]`); leave out for an instrumental. */
    lyrics?: string;
    durationSeconds?: number;
    seed?: number;
    onProgress?: (p: ProgressInfo) => void;
    signal?: AbortSignal;
  }): Promise<GeneratedFile>;
  /** The chat model text calls use now. */
  info(): Promise<{
    textModel: string | null;
  }>;
};
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
  pages: Array<{
    url: string;
    title: string | null;
    content: string;
    depth: number;
  }>;
  skipped: Array<{
    url: string;
    reason: string;
  }>;
  /** The site had a robots.txt (which was followed). */
  robots: boolean;
}
export declare const web: {
  /** Searches the web with the search service set in Settings → Tools. */
  search(
    query: string,
    o?: {
      count?: number;
      signal?: AbortSignal;
    },
  ): Promise<SearchHit[]>;
  /** Reads a page as Markdown, or as its HTML with `format: "html"`. */
  fetch(
    url: string,
    o?: {
      format?: "markdown" | "html";
      signal?: AbortSignal;
    },
  ): Promise<Page>;
  /** Reads pages of one site, breadth first, following its robots.txt (at most 50 pages). */
  crawl(
    url: string,
    o?: {
      maxPages?: number;
      maxDepth?: number;
      format?: "markdown" | "html";
      signal?: AbortSignal;
    },
  ): Promise<CrawlResult>;
};
export interface ToolInfo {
  name: string;
  description: string | null;
  inputSchema: unknown;
}
export interface ToolResult {
  text: string;
  images: Array<{
    mediaType: string;
    data: string;
  }>;
  structured: unknown;
  isError: boolean;
}
export declare const connectors: {
  /** The tools of the MCP servers that are running. */
  list(): Promise<
    Array<
      ToolInfo & {
        server: string;
        title: string | null;
        readOnly: boolean;
      }
    >
  >;
  call(server: string, tool: string, args?: Record<string, unknown>): Promise<ToolResult>;
};
export declare const python: {
  /** Runs Python in Oxide's managed environment (in a working folder of the app's own). */
  run(
    code: string,
    o?: {
      timeoutSeconds?: number;
      signal?: AbortSignal;
    },
  ): Promise<{
    exitCode: number | null;
    stdout: string;
    stderr: string;
    timedOut: boolean;
  }>;
};
export declare const sandbox: {
  /** Runs a shell command in the app's Docker sandbox (`/workspace`). */
  exec(
    command: string,
    o?: {
      cwd?: string;
      timeoutSeconds?: number;
      signal?: AbortSignal;
    },
  ): Promise<{
    exitCode: number | null;
    stdout: string;
    stderr: string;
    timedOut: boolean;
    cwd: string;
  }>;
};
export declare const browser: {
  /** The browser agent's tools (`navigate`, `read_page`, `browser_task`, …). */
  tools(): Promise<ToolInfo[]>;
  call(tool: string, args?: Record<string, unknown>): Promise<ToolResult>;
};
export declare const chrome: {
  /** The paired Chrome's tools (`navigate`, `read_page`, `click`, …). */
  tools(): Promise<ToolInfo[]>;
  call(tool: string, args?: Record<string, unknown>): Promise<ToolResult>;
};
export declare const files: {
  /** Lets the user pick files (`accept` like an `<input type=file>`'s). Empty when cancelled. */
  pick(o?: { accept?: string; multiple?: boolean }): Promise<File[]>;
  /** Saves a file where the user picks. Resolves to whether it was saved. */
  save(
    name: string,
    data: Blob | string | Uint8Array,
    o?: {
      mediaType?: string;
    },
  ): Promise<boolean>;
  /** A zip of the given files (made in the frame with JSZip). */
  zip(
    entries: Array<{
      name: string;
      data: Blob | string | Uint8Array;
    }>,
  ): Promise<Blob>;
  readText(file: Blob): Promise<string>;
  toBase64: typeof toBase64;
  toDataUrl: typeof toDataUrl;
};
/** The app's own storage, kept by Oxide (values are JSON). */
export declare const storage: {
  get<T = unknown>(key: string): Promise<T | undefined>;
  set(key: string, value: unknown): Promise<void>;
  delete(key: string): Promise<void>;
  keys(): Promise<string[]>;
  clear(): Promise<void>;
};
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
/**
 * One folder on this computer that the user picks for the app (declare `"folder"` in the
 * manifest). Paths are relative to it and `/`-separated; nothing outside it can be reached.
 * Picking the folder allows the app to use it; until then, calls other than `get` and
 * `choose` fail.
 */
export declare const folder: {
  /** The app's folder, or `null` when none is chosen yet. */
  get(): Promise<FolderInfo | null>;
  /** Lets the user pick the app's folder (the system's folder picker). `null` when cancelled. */
  choose(): Promise<FolderInfo | null>;
  /** What a folder holds (`""`: the app's folder), `depth` levels deep (default 1, at most 12). */
  list(
    path?: string,
    o?: {
      depth?: number;
    },
  ): Promise<FolderEntry[]>;
  /** A text file's content (UTF-8). */
  readText(path: string): Promise<string>;
  /** A file's bytes. */
  readBytes(path: string): Promise<Uint8Array>;
  /** Writes a file (making its folders), replacing what was there. At most 20 MB. */
  write(path: string, data: string | Blob | Uint8Array): Promise<void>;
  /** Makes a folder (and its parents). */
  mkdir(path: string): Promise<void>;
  /** Deletes a file, or a folder with everything in it, for good. */
  delete(path: string): Promise<void>;
  /** Renames or moves a file or folder (`to` must not exist yet). */
  rename(from: string, to: string): Promise<void>;
};
export declare const app: {
  readonly id: string;
  readonly name: string;
  /** Oxide's theme; the frame's `<html>` has the class `dark` in the dark theme. */
  readonly theme: "light" | "dark";
  /** Copies text to the clipboard. */
  copy(text: string): Promise<void>;
};
export {};
