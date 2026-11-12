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
  | "connector";

export function toolKind(call: Pick<ToolCallView, "tool" | "server">): ToolKind {
  if (call.tool === "web_search" && (call.server === null || call.server === "anthropic")) {
    return "search";
  }
  if (call.tool === "web_fetch" && call.server === null) return "fetch";
  if (call.tool === "project_search" && call.server === null) return "project";
  if (call.tool === "artifact" && call.server === null) return "artifact";
  if (call.tool === "load_skill" && call.server === null) return "skill";
  if (call.server === null && call.tool in FILE_TOOL_VERBS) return "file";
  if (call.server === null && call.tool === "task_list") return "tasks";
  if (call.server === null && call.tool === "run_command") return "shell";
  if (call.server === null && call.tool === "run_python") return "python";
  if (isSandboxCall(call)) return "sandbox";
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
