import type { ContentBlock, ToolUsage } from "@/bindings";
import type { ToolCallView } from "@/features/chat/streams";

/** A saved reply as the UI draws it: text, thinking and tool calls in order. */
export type ReplySegment =
  | { kind: "text"; text: string }
  | { kind: "thinking"; text: string; seconds: number }
  | { kind: "tool"; call: ToolCallView };

/** A short summary of a provider tool result (Anthropic's web search). */
export function providerResultBlocks(rawJson: string): {
  content: ContentBlock[];
  isError: boolean;
} {
  try {
    const v = JSON.parse(rawJson) as { content?: unknown };
    if (Array.isArray(v.content)) {
      const lines = v.content
        .map(
          (r: { title?: string; url?: string }, i) =>
            `${i + 1}. ${r.title ?? ""}\n   ${r.url ?? ""}`,
        )
        .join("\n");
      return { content: [{ type: "text", text: lines || "No results." }], isError: false };
    }
    if (v.content && typeof v.content === "object") {
      const code = (v.content as { error_code?: string }).error_code ?? "unknown";
      return { content: [{ type: "text", text: `The search failed (${code}).` }], isError: true };
    }
  } catch {
    // fall through
  }
  return { content: [{ type: "text", text: "(no result)" }], isError: false };
}

/** Splits a saved reply into what's drawn, pairing tool calls with their results. */
export function replySegments(content: ContentBlock[]): ReplySegment[] {
  const results = new Map<string, Extract<ContentBlock, { type: "toolResult" }>>();
  const serverResults = new Map<string, string>();
  for (const b of content) {
    if (b.type === "toolResult") results.set(b.toolUseId, b);
    if (b.type === "serverToolResult") serverResults.set(b.toolUseId, b.rawJson);
  }
  const out: ReplySegment[] = [];
  for (const b of content) {
    switch (b.type) {
      case "text": {
        if (b.text.trim() === "") break;
        const last = out[out.length - 1];
        if (last?.kind === "text") last.text += `\n\n${b.text}`;
        else out.push({ kind: "text", text: b.text });
        break;
      }
      case "thinking":
        if (b.text.trim() === "") break;
        out.push({
          kind: "thinking",
          text: b.text,
          seconds: Math.max(1, Math.round((b.durationMs ?? 0) / 1000)),
        });
        break;
      case "toolUse": {
        const r = results.get(b.id);
        out.push({
          kind: "tool",
          call: {
            id: b.id,
            name: b.name,
            server: b.server ?? null,
            tool: b.tool ?? b.name,
            title: null,
            inputJson: b.inputJson,
            status: r ? (r.isError ? "error" : "done") : "done",
            approval: null,
            result: r?.content ?? null,
            isError: r?.isError ?? false,
            durationMs: null,
            provider: false,
          },
        });
        break;
      }
      case "serverToolUse": {
        const raw = serverResults.get(b.id);
        const r = raw ? providerResultBlocks(raw) : null;
        out.push({
          kind: "tool",
          call: {
            id: b.id,
            name: b.name,
            server: "anthropic",
            tool: b.name,
            title: b.name === "web_search" ? "Web search" : b.name,
            inputJson: b.inputJson,
            status: r?.isError ? "error" : "done",
            approval: null,
            result: r?.content ?? null,
            isError: r?.isError ?? false,
            durationMs: null,
            provider: true,
          },
        });
        break;
      }
      default:
        break;
    }
  }
  return out;
}

function parse(json: string): Record<string, unknown> {
  try {
    const v: unknown = JSON.parse(json);
    return v && typeof v === "object" && !Array.isArray(v) ? (v as Record<string, unknown>) : {};
  } catch {
    return {};
  }
}

/** Pretty-printed arguments (the raw text if it isn't JSON). */
export function prettyJson(json: string): string {
  try {
    return JSON.stringify(JSON.parse(json), null, 2);
  } catch {
    return json;
  }
}

function hostOf(url: string): string {
  try {
    return new URL(url).host;
  } catch {
    return url;
  }
}

const LABEL_KEYS = [
  "path",
  "file",
  "filename",
  "url",
  "uri",
  "query",
  "q",
  "name",
  "title",
  "command",
];

/** The first non-empty line of a script, cut to 80 characters (as the backend labels it). */
export function codeLabel(code: string): string {
  const line =
    code
      .split("\n")
      .map((l) => l.trim())
      .find((l) => l !== "") ?? "";
  const chars = [...line];
  return chars.length > 80 ? `${chars.slice(0, 80).join("")}…` : line;
}

/** The script of a `run_python` call (empty until the arguments are complete JSON). */
export function pythonCode(inputJson: string): string {
  const code = parse(inputJson).code;
  return typeof code === "string" ? code : "";
}

/** Oxide's Docker sandbox tools (Phase 12b): verbs while running and after. */
export const SANDBOX_TOOL_VERBS: Record<string, [string, string]> = {
  sandbox_exec: ["Running in the sandbox", "Ran in the sandbox"],
  sandbox_copy_in: ["Copying into the sandbox", "Copied into the sandbox"],
  sandbox_copy_out: ["Copying out of the sandbox", "Copied out of the sandbox"],
};

/** The browser agent's tools (Phase 13): verbs while running and after. */
export const BROWSER_TOOL_VERBS: Record<string, [string, string]> = {
  browser_task: ["Browsing", "Browsed"],
  navigate: ["Opening", "Opened"],
  search: ["Searching Google for", "Searched Google for"],
  read_page: ["Reading the page", "Read the page"],
  click: ["Clicking", "Clicked"],
  type: ["Typing", "Typed"],
  screenshot: ["Taking a screenshot", "Took a screenshot"],
  close: ["Closing the browser", "Closed the browser"],
};

/** The Chrome extension's tools (Phase 14): verbs while running and after. */
export const CHROME_TOOL_VERBS: Record<string, [string, string]> = {
  tabs_list: ["Listing Chrome tabs", "Listed Chrome tabs"],
  tabs_create: ["Opening a tab in Chrome", "Opened a tab in Chrome"],
  tabs_select: ["Switching to a Chrome tab", "Switched to a Chrome tab"],
  tabs_close: ["Closing a Chrome tab", "Closed a Chrome tab"],
  navigate: ["Opening in Chrome", "Opened in Chrome"],
  read_page: ["Reading the page in Chrome", "Read the page in Chrome"],
  find: ["Finding in Chrome", "Found in Chrome"],
  click: ["Clicking in Chrome", "Clicked in Chrome"],
  type: ["Typing in Chrome", "Typed in Chrome"],
  press_key: ["Pressing a key in Chrome", "Pressed a key in Chrome"],
  scroll: ["Scrolling in Chrome", "Scrolled in Chrome"],
  form_input: ["Filling in a form in Chrome", "Filled in a form in Chrome"],
  screenshot: ["Taking a screenshot in Chrome", "Took a screenshot in Chrome"],
  get_console: ["Reading Chrome's console", "Read Chrome's console"],
};

/** Oxide's generation tools (Phase 19): verbs while running and after. */
export const GENERATION_TOOL_VERBS: Record<string, [string, string]> = {
  generate_image: ["Making an image", "Made an image"],
  generate_video: ["Making a video", "Made a video"],
  generate_audio: ["Reading aloud", "Read aloud"],
};

/** Whether a call is one of the generation tools. */
export function isGenerationCall(call: Pick<ToolCallView, "tool" | "server">): boolean {
  return call.server === null && call.tool in GENERATION_TOOL_VERBS;
}

/** Whether a call is one of the Chrome extension's. */
export function isChromeCall(call: Pick<ToolCallView, "tool" | "server">): boolean {
  return call.server === "chrome" && call.tool in CHROME_TOOL_VERBS;
}

/** Whether a call is one of the browser agent's. */
export function isBrowserCall(call: Pick<ToolCallView, "tool" | "server">): boolean {
  return call.server === "browser" && call.tool in BROWSER_TOOL_VERBS;
}

/** The goal of a `browser_task` call (empty until the arguments are complete JSON). */
export function browserGoal(inputJson: string): string {
  const g = parse(inputJson).goal;
  return typeof g === "string" ? g : "";
}

/** The sandbox file server's tools that have their own verbs. */
const SANDBOX_FILE_VERBS: Record<string, [string, string]> = {
  read_text_file: ["Reading", "Read"],
  read_media_file: ["Reading", "Read"],
  read_multiple_files: ["Reading", "Read"],
  write_file: ["Writing", "Wrote"],
  edit_file: ["Editing", "Edited"],
  create_directory: ["Creating folder", "Created folder"],
  list_directory: ["Listing", "Listed"],
  list_directory_with_sizes: ["Listing", "Listed"],
  directory_tree: ["Listing", "Listed"],
  move_file: ["Moving", "Moved"],
  copy_file: ["Copying", "Copied"],
  search_files: ["Searching", "Searched"],
  get_file_info: ["Checking", "Checked"],
};

/** Whether a call is one of the sandbox's: its own tools, or its file server's. */
export function isSandboxCall(call: Pick<ToolCallView, "tool" | "server">): boolean {
  return (
    (call.server === null && call.tool in SANDBOX_TOOL_VERBS) ||
    (call.server === "sandbox" && call.tool in SANDBOX_FILE_VERBS)
  );
}

/** The command of a `sandbox_exec` call (empty until the arguments are complete JSON). */
export function sandboxCommand(inputJson: string): string {
  const c = parse(inputJson).command;
  return typeof c === "string" ? c : "";
}

/** What a tool call was about: the query, URL or path. */
export function callDetail(call: Pick<ToolCallView, "tool" | "server" | "inputJson">): string {
  const input = parse(call.inputJson);
  const str = (k: string) => {
    const v = input[k];
    return typeof v === "string" && v.trim() !== "" ? v.trim() : null;
  };
  if (call.tool === "web_search") return str("query") ?? "";
  if (isGenerationCall(call)) {
    const text = str("prompt") ?? str("text") ?? "";
    const line = text.split("\n")[0] ?? "";
    return line.length > 80 ? `${line.slice(0, 80)}…` : line;
  }
  if (call.tool === "web_fetch") return str("url") ?? "";
  if (call.tool === "project_search") return str("query") ?? "";
  if (call.server === null && call.tool === "move_file") {
    const from = str("from") ?? str("source") ?? "";
    const to = str("to") ?? str("destination") ?? "";
    return from && to ? `${from} → ${to}` : from || to;
  }
  if (call.server === null && call.tool === "search_files") {
    return str("text") ?? str("query") ?? str("name") ?? "";
  }
  if (call.server === null && call.tool === "task_list") return taskDetail(input);
  if (call.server === null && call.tool === "run_python") return codeLabel(str("code") ?? "");
  if (call.server === null && call.tool === "sandbox_exec") return codeLabel(str("command") ?? "");
  if (call.server === null && call.tool.startsWith("sandbox_copy")) {
    const from = str("from") ?? "";
    const to = str("to");
    return to ? `${from} → ${to}` : from;
  }
  if (isBrowserCall(call)) {
    if (call.tool === "browser_task") return str("goal") ?? "";
    if (call.tool === "navigate") return str("url") ?? "";
    if (call.tool === "search") return str("query") ?? "";
    if (call.tool === "click") {
      const ref = input.ref;
      return typeof ref === "number" ? `[${ref}]` : (str("text") ?? "");
    }
    if (call.tool === "type") {
      const t = str("text");
      return t ? `“${t}”` : "";
    }
    return "";
  }
  if (isChromeCall(call)) {
    const ref = typeof input.ref === "number" ? `[${input.ref}]` : null;
    const tab = typeof input.tab_id === "number" ? `tab ${input.tab_id}` : null;
    switch (call.tool) {
      case "navigate":
      case "tabs_create":
        return str("url") ?? "";
      case "find":
        return str("query") ?? "";
      case "click":
        return ref ?? str("text") ?? "";
      case "type": {
        const t = str("text");
        return t ? `“${t}”` : "";
      }
      case "press_key":
        return str("key") ?? "";
      case "form_input":
        return ref ?? "";
      case "tabs_select":
      case "tabs_close":
        return tab ?? "";
      default:
        return "";
    }
  }
  if (call.server === null && call.tool === "save_skill") return savedSkillFolder(call.inputJson);
  if (call.tool === "load_skill") {
    const name = str("name") ?? "";
    const path = str("path");
    return path ? `${name} · ${path}` : name;
  }
  for (const k of LABEL_KEYS) {
    const v = str(k);
    if (v) return v;
  }
  const first = Object.values(input).find((v) => typeof v === "string" && v.trim() !== "");
  return typeof first === "string" ? first : "";
}

/** A skill name as Oxide stores it (as `skill_slug` in Rust): lowercase letters, digits, hyphens. */
export function skillSlug(name: string): string {
  let out = "";
  for (const c of name.trim()) {
    if (/^[A-Za-z0-9]$/.test(c)) out += c.toLowerCase();
    else if (out !== "" && !out.endsWith("-")) out += "-";
  }
  return out
    .replace(/^-+|-+$/g, "")
    .slice(0, 64)
    .replace(/-+$/, "");
}

/** A `save_skill` call changes an existing skill (it names one in `skill`). */
export function isSkillChange(inputJson: string): boolean {
  const k = parse(inputJson).skill;
  return typeof k === "string" && k.trim() !== "";
}

/** The folder a `save_skill` call writes: the skill it changes, or the new name's. */
export function savedSkillFolder(inputJson: string): string {
  const input = parse(inputJson);
  if (typeof input.skill === "string" && input.skill.trim() !== "") return input.skill.trim();
  return typeof input.name === "string" ? skillSlug(input.name) : "";
}

/** "Now: <step>" or "n of m done" for a task_list call. */
function taskDetail(input: Record<string, unknown>): string {
  const list = input.tasks ?? input.todos ?? input.items ?? input.steps;
  if (!Array.isArray(list) || list.length === 0) return "";
  const steps = list.map((t: unknown) => {
    if (typeof t === "string") return { title: t, status: "pending" };
    const o = (t ?? {}) as Record<string, unknown>;
    const title = [o.title, o.content, o.name, o.step].find((v) => typeof v === "string");
    const status = typeof o.status === "string" ? o.status.toLowerCase() : "pending";
    return { title: (title as string | undefined) ?? "", status };
  });
  const isDone = (st: string) => ["done", "completed", "complete", "finished"].includes(st);
  const done = steps.filter((t) => isDone(t.status)).length;
  return `${done} of ${steps.length} done`;
}

/** Oxide's file tools (Phase 11): verbs while running and after. */
export const FILE_TOOL_VERBS: Record<string, [string, string]> = {
  list_dir: ["Listing", "Listed"],
  read_file: ["Reading", "Read"],
  search_files: ["Searching files for", "Searched files for"],
  write_file: ["Writing", "Wrote"],
  edit_file: ["Editing", "Edited"],
  move_file: ["Moving", "Moved"],
  delete_file: ["Deleting", "Deleted"],
};

export type ToolKind =
  | "search"
  | "fetch"
  | "project"
  | "artifact"
  | "skill"
  | "file"
  | "tasks"
  | "shell"
  | "python"
  | "sandbox"
  | "browser"
  | "chrome"
  | "computer"
  | "generation"
  | "connector";

export function toolKind(call: Pick<ToolCallView, "tool" | "server">): ToolKind {
  if (call.tool === "web_search" && (call.server === null || call.server === "anthropic")) {
    return "search";
  }
  if (call.tool === "web_fetch" && call.server === null) return "fetch";
  if (call.tool === "project_search" && call.server === null) return "project";
  if (call.tool === "artifact" && call.server === null) return "artifact";
  if (call.tool === "load_skill" && call.server === null) return "skill";
  if (call.tool === "save_skill" && call.server === null) return "skill";
  if (call.server === null && call.tool in FILE_TOOL_VERBS) return "file";
  if (call.server === null && call.tool === "task_list") return "tasks";
  if (call.server === null && call.tool === "run_command") return "shell";
  if (call.server === null && call.tool === "run_python") return "python";
  if (isSandboxCall(call)) return "sandbox";
  if (isBrowserCall(call)) return "browser";
  if (isChromeCall(call)) return "chrome";
  if (call.server === "computer" && call.tool === "computer") return "computer";
  if (isGenerationCall(call)) return "generation";
  return "connector";
}

/** The line shown for a call: a verb phrase and a detail. */
export function callHeadline(call: ToolCallView): { action: string; detail: string } {
  const detail = callDetail(call);
  const busy =
    call.status === "streaming" ||
    call.status === "ready" ||
    call.status === "approved" ||
    call.status === "awaitingApproval" ||
    call.status === "running";
  switch (toolKind(call)) {
    case "search":
      return { action: busy ? "Searching the web" : "Searched the web", detail };
    case "fetch":
      return { action: busy ? "Reading" : "Read", detail: detail ? hostOf(detail) : "a web page" };
    case "project":
      return {
        action: busy ? "Searching project knowledge" : "Searched project knowledge",
        detail,
      };
    case "artifact":
      return { action: busy ? "Writing artifact" : "Artifact", detail };
    case "skill":
      if (call.tool === "save_skill") {
        const change = isSkillChange(call.inputJson);
        if (busy) return { action: change ? "Updating skill" : "Creating skill", detail };
        if (call.status === "error") return { action: "Save skill", detail };
        return { action: change ? "Updated skill" : "Created skill", detail };
      }
      return { action: busy ? "Loading skill" : "Loaded skill", detail };
    case "file": {
      const [doing, did] = FILE_TOOL_VERBS[call.tool] ?? ["Using", "Used"];
      return { action: busy ? doing : did, detail: detail || "folder" };
    }
    case "tasks":
      return { action: busy ? "Updating task list" : "Updated task list", detail };
    case "shell":
      return { action: busy ? "Running" : "Ran", detail };
    case "python":
      return { action: busy ? "Running Python" : "Ran Python", detail };
    case "sandbox": {
      const own = SANDBOX_TOOL_VERBS[call.tool];
      if (own) return { action: busy ? own[0] : own[1], detail };
      const [doing, did] = SANDBOX_FILE_VERBS[call.tool] ?? ["Using", "Used"];
      return {
        action: `${busy ? doing : did} in the sandbox`,
        detail: detail.replace(/^\/workspace\/?/, "") || "/workspace",
      };
    }
    case "browser": {
      const [doing, did] = BROWSER_TOOL_VERBS[call.tool] ?? ["Browsing", "Browsed"];
      return { action: busy ? doing : did, detail };
    }
    case "chrome": {
      const [doing, did] = CHROME_TOOL_VERBS[call.tool] ?? ["Using Chrome", "Used Chrome"];
      return { action: busy ? doing : did, detail };
    }
    case "computer": {
      const [doing, did] = computerVerbs(call.inputJson);
      return { action: busy ? doing : did, detail: computerDetail(call.inputJson) };
    }
    case "generation": {
      const [doing, did] = GENERATION_TOOL_VERBS[call.tool] ?? ["Generating", "Generated"];
      return { action: busy ? doing : did, detail };
    }
    case "connector": {
      const name = call.title ?? call.tool;
      return { action: call.server ? `${name}` : name, detail };
    }
  }
}

/** Rows of "Used in this chat": repeated uses of the same thing collapse with a count. */
export interface UsageRow {
  key: string;
  kind: ToolUsage["kind"];
  title: string;
  /** Labels (queries, hosts, paths), newest last, without repeats. */
  details: string[];
  count: number;
  errors: number;
  /** The first call, to scroll to. */
  toolUseId: string;
}

export function usageRows(usage: ToolUsage[]): UsageRow[] {
  const rows = new Map<string, UsageRow>();
  for (const u of usage) {
    // The task list has its own section ("Progress").
    if (u.kind === "tasks") continue;
    const key =
      u.kind === "connector"
        ? `connector:${u.server ?? ""}`
        : u.kind === "webSearch"
          ? "search"
          : u.kind;
    const title =
      u.kind === "webSearch"
        ? "Web search"
        : u.kind === "webFetch"
          ? "Web pages"
          : u.kind === "providerTool"
            ? u.tool
            : u.kind === "projectSearch"
              ? "Project knowledge"
              : u.kind === "artifact"
                ? "Artifacts"
                : u.kind === "skill"
                  ? "Skills"
                  : u.kind === "file"
                    ? "Files"
                    : u.kind === "shell"
                      ? "Commands"
                      : u.kind === "python"
                        ? "Python"
                        : u.kind === "sandbox"
                          ? "Sandbox"
                          : u.kind === "browser"
                            ? "Browser"
                            : u.kind === "chrome"
                              ? "Chrome"
                              : u.kind === "computer"
                                ? "Computer"
                                : u.kind === "generation"
                                  ? "Generated media"
                                  : (u.server ?? "Connector");
    const row = rows.get(key) ?? {
      key,
      kind: u.kind,
      title,
      details: [],
      count: 0,
      errors: 0,
      toolUseId: u.toolUseId,
    };
    row.count += 1;
    if (u.isError) row.errors += 1;
    const detail =
      u.kind === "webFetch" && u.label
        ? hostOf(u.label)
        : u.kind === "connector"
          ? u.tool
          : (u.label ?? "");
    if (detail && !row.details.includes(detail)) row.details.push(detail);
    rows.set(key, row);
  }
  return [...rows.values()];
}

// ---- Computer use ----------------------------------------------------------------------------

const COMPUTER_VERBS: Record<string, [string, string]> = {
  screenshot: ["Taking a screenshot", "Took a screenshot"],
  zoom: ["Zooming in", "Zoomed in"],
  left_click: ["Clicking", "Clicked"],
  click: ["Clicking", "Clicked"],
  right_click: ["Right-clicking", "Right-clicked"],
  middle_click: ["Middle-clicking", "Middle-clicked"],
  double_click: ["Double-clicking", "Double-clicked"],
  triple_click: ["Triple-clicking", "Triple-clicked"],
  left_click_drag: ["Dragging", "Dragged"],
  drag: ["Dragging", "Dragged"],
  mouse_move: ["Moving the mouse", "Moved the mouse"],
  left_mouse_down: ["Pressing the mouse button", "Pressed the mouse button"],
  left_mouse_up: ["Releasing the mouse button", "Released the mouse button"],
  cursor_position: ["Finding the mouse pointer", "Found the mouse pointer"],
  scroll: ["Scrolling", "Scrolled"],
  type: ["Typing", "Typed"],
  key: ["Pressing", "Pressed"],
  hold_key: ["Holding", "Held"],
  wait: ["Waiting", "Waited"],
};

/** The action a computer call asks for (`left_click`, `type`, …). */
export function computerAction(json: string): string {
  const a = parse(json).action;
  return typeof a === "string" ? a.trim() : "";
}

/** "Clicking" / "Clicked" for a computer call. */
export function computerVerbs(json: string): [string, string] {
  return COMPUTER_VERBS[computerAction(json)] ?? ["Using the computer", "Used the computer"];
}

function pointOf(v: unknown): string {
  if (Array.isArray(v) && v.length === 2) return `(${String(v[0])}, ${String(v[1])})`;
  return "";
}

/** What a computer call acts on: where it clicks, what it types or presses. */
export function computerDetail(json: string): string {
  const input = parse(json);
  const action = computerAction(json);
  const text = typeof input.text === "string" ? input.text : "";
  switch (action) {
    case "type": {
      const short = text.replace(/\n/g, " ");
      return `“${short.length > 40 ? `${short.slice(0, 40)}…` : short}”`;
    }
    case "key":
    case "hold_key":
      return text;
    case "scroll": {
      const dir = typeof input.scroll_direction === "string" ? input.scroll_direction : "down";
      const at = pointOf(input.coordinate);
      return at ? `${dir} at ${at}` : dir;
    }
    case "left_click_drag":
    case "drag": {
      const from = pointOf(input.start_coordinate);
      const to = pointOf(input.coordinate);
      return from && to ? `${from} → ${to}` : to;
    }
    case "wait": {
      const d = typeof input.duration === "number" ? input.duration : 1;
      return `${d} s`;
    }
    default: {
      const at = pointOf(input.coordinate);
      return at ? `at ${at}` : "";
    }
  }
}
