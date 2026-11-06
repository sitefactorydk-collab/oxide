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

/** What a call was about: the query, URL or path. */
export function callDetail(call: Pick<ToolCallView, "tool" | "server" | "inputJson">): string {
  const input = parse(call.inputJson);
  const str = (k: string) => {
    const v = input[k];
    return typeof v === "string" && v.trim() !== "" ? v.trim() : null;
  };
  if (call.tool === "web_search") return str("query") ?? "";
  if (call.tool === "web_fetch") return str("url") ?? "";
  for (const k of LABEL_KEYS) {
    const v = str(k);
    if (v) return v;
  }
  const first = Object.values(input).find((v) => typeof v === "string" && v.trim() !== "");
  return typeof first === "string" ? first : "";
}

export type ToolKind = "search" | "fetch" | "connector";

export function toolKind(call: Pick<ToolCallView, "tool" | "server">): ToolKind {
  if (call.tool === "web_search" && (call.server === null || call.server === "anthropic")) {
    return "search";
  }
  if (call.tool === "web_fetch" && call.server === null) return "fetch";
  return "connector";
}

/** The line shown for a call: a verb phrase and a detail. */
export function callHeadline(call: ToolCallView): { action: string; detail: string } {
  const detail = callDetail(call);
  const busy = call.status === "streaming" || call.status === "ready" || call.status === "running";
  switch (toolKind(call)) {
    case "search":
      return { action: busy ? "Searching the web" : "Searched the web", detail };
    case "fetch":
      return { action: busy ? "Reading" : "Read", detail: detail ? hostOf(detail) : "a web page" };
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
