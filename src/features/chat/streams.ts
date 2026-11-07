import { create } from "zustand";
import type {
  ChatStreamEvent,
  ContentBlock,
  Message,
  ToolApprovalRequest,
  Usage,
} from "@/bindings";

/** Where a tool call is. */
export type ToolStatus =
  /** The model is still writing the arguments. */
  | "streaming"
  /** Arguments complete; about to be checked or run. */
  | "ready"
  /** Waiting for the user to allow or deny it. */
  | "awaitingApproval"
  /** Allowed; runs once the other calls of the step are answered. */
  | "approved"
  | "running"
  | "done"
  | "error";

/** A tool call as the UI shows it, live or saved. */
export interface ToolCallView {
  id: string;
  /** The name the model used (`server__tool` for MCP tools). */
  name: string;
  /** MCP server; `null` for Oxide's own tools; `"anthropic"` for provider-run tools. */
  server: string | null;
  tool: string;
  title: string | null;
  inputJson: string;
  status: ToolStatus;
  approval: ToolApprovalRequest | null;
  result: ContentBlock[] | null;
  isError: boolean;
  durationMs: number | null;
  /** Provider-run (Anthropic web search). */
  provider: boolean;
}

/** One piece of a live reply, in the order it arrived. */
export type LiveBlock =
  | { kind: "text"; text: string }
  | { kind: "thinking"; text: string; startedAt: number; endedAt: number | null }
  | { kind: "tool"; call: ToolCallView };

/** A reply being written, built from `chatStream` events. */
export interface LiveReply {
  messageId: string;
  /** The user message being answered. */
  parentId: string;
  model: string;
  startedAt: number;
  blocks: LiveBlock[];
  usage: Usage | null;
}

export interface ReplyFailure {
  error: string;
  /** The partial reply that was saved, if any. */
  message: Message | null;
}

interface StreamsState {
  /** Live replies by conversation id. */
  live: Record<string, LiveReply>;
  /** The last failed reply by conversation id (cleared when a new reply starts). */
  failures: Record<string, ReplyFailure>;
  apply: (e: ChatStreamEvent, now?: number) => void;
  clearFailure: (conversationId: string) => void;
  /** Closes an approval card as soon as the user answers it. */
  markAnswered: (conversationId: string, toolUseId: string, allowed: boolean) => void;
  /** "Allow always": closes every waiting card of the same tool in the chat. */
  markToolAllowed: (conversationId: string, server: string | null, tool: string) => void;
}

type Maps = Pick<StreamsState, "live" | "failures">;

function without<T>(map: Record<string, T>, key: string): Record<string, T> {
  if (!(key in map)) return map;
  const next = { ...map };
  delete next[key];
  return next;
}

/** Closes an open thinking block. */
function endThinking(blocks: LiveBlock[], now: number): LiveBlock[] {
  const last = blocks[blocks.length - 1];
  if (last?.kind === "thinking" && last.endedAt === null) {
    return [...blocks.slice(0, -1), { ...last, endedAt: now }];
  }
  return blocks;
}

function updateTool(
  blocks: LiveBlock[],
  id: string,
  f: (c: ToolCallView) => ToolCallView,
): LiveBlock[] {
  return blocks.map((b) => (b.kind === "tool" && b.call.id === id ? { ...b, call: f(b.call) } : b));
}

/** All text of a live reply (for tests and the status line). */
export function liveText(live: LiveReply): string {
  return live.blocks.flatMap((b) => (b.kind === "text" ? [b.text] : [])).join("");
}

/** All thinking of a live reply. */
export function liveThinking(live: LiveReply): string {
  return live.blocks.flatMap((b) => (b.kind === "thinking" ? [b.text] : [])).join("\n\n");
}

/** Applies one stream event. Pure, so it can be tested on its own. */
export function reduceStream(
  state: Maps,
  { conversationId: id, event }: ChatStreamEvent,
  now: number,
): Maps {
  const live = state.live[id];
  const put = (next: LiveReply): Maps => ({ ...state, live: { ...state.live, [id]: next } });
  switch (event.type) {
    case "started":
      return {
        live: {
          ...state.live,
          [id]: {
            messageId: event.messageId,
            parentId: event.parentId,
            model: event.model,
            startedAt: now,
            blocks: [],
            usage: null,
          },
        },
        failures: without(state.failures, id),
      };
    case "thinkingDelta": {
      if (!live) return state;
      const last = live.blocks[live.blocks.length - 1];
      const blocks: LiveBlock[] =
        last?.kind === "thinking" && last.endedAt === null
          ? [...live.blocks.slice(0, -1), { ...last, text: last.text + event.text }]
          : [...live.blocks, { kind: "thinking", text: event.text, startedAt: now, endedAt: null }];
      return put({ ...live, blocks });
    }
    case "textDelta": {
      if (!live) return state;
      const closed = endThinking(live.blocks, now);
      const last = closed[closed.length - 1];
      const blocks: LiveBlock[] =
        last?.kind === "text"
          ? [...closed.slice(0, -1), { kind: "text", text: last.text + event.text }]
          : [...closed, { kind: "text", text: event.text }];
      return put({ ...live, blocks });
    }
    case "toolUse": {
      if (!live) return state;
      const call: ToolCallView = {
        id: event.id,
        name: event.name,
        server: null,
        // `server__tool` until the backend says which server it is.
        tool: event.name.includes("__") ? event.name.split("__").slice(1).join("__") : event.name,
        title: null,
        inputJson: "",
        status: "streaming",
        approval: null,
        result: null,
        isError: false,
        durationMs: null,
        provider: false,
      };
      return put({ ...live, blocks: [...endThinking(live.blocks, now), { kind: "tool", call }] });
    }
    case "toolInputDelta":
      if (!live) return state;
      return put({
        ...live,
        blocks: updateTool(live.blocks, event.id, (c) =>
          c.status === "streaming" ? { ...c, inputJson: c.inputJson + event.partialJson } : c,
        ),
      });
    case "toolCall":
      if (!live) return state;
      return put({
        ...live,
        blocks: updateTool(live.blocks, event.id, (c) => ({
          ...c,
          server: event.server,
          tool: event.tool,
          title: event.title,
          inputJson: event.inputJson,
          provider: event.server === "anthropic",
          status: c.status === "streaming" ? "ready" : c.status,
        })),
      });
    case "toolApproval":
      if (!live) return state;
      return put({
        ...live,
        blocks: updateTool(live.blocks, event.request.toolUseId, (c) => ({
          ...c,
          status: "awaitingApproval",
          approval: event.request,
        })),
      });
    case "toolRunning":
      if (!live) return state;
      return put({
        ...live,
        blocks: updateTool(live.blocks, event.id, (c) => ({
          ...c,
          status: "running",
          approval: null,
        })),
      });
    case "toolResult":
      if (!live) return state;
      return put({
        ...live,
        blocks: updateTool(live.blocks, event.id, (c) => ({
          ...c,
          status: event.isError ? "error" : "done",
          approval: null,
          result: event.content,
          isError: event.isError,
          durationMs: event.durationMs,
        })),
      });
    case "usage":
      if (!live) return state;
      return put({ ...live, usage: event.usage });
    case "done":
      return { live: without(state.live, id), failures: without(state.failures, id) };
    case "failed":
      return {
        live: without(state.live, id),
        failures: { ...state.failures, [id]: { error: event.error, message: event.message } },
      };
  }
}

export const useStreams = create<StreamsState>()((set) => ({
  live: {},
  failures: {},
  apply: (e, now = Date.now()) => set((s) => reduceStream(s, e, now)),
  clearFailure: (id) => set((s) => ({ failures: without(s.failures, id) })),
  markAnswered: (id, toolUseId, allowed) =>
    set((s) => {
      const live = s.live[id];
      if (!live) return s;
      const blocks = updateTool(live.blocks, toolUseId, (c) =>
        c.status === "awaitingApproval"
          ? { ...c, approval: null, status: allowed ? "approved" : "ready" }
          : c,
      );
      return { live: { ...s.live, [id]: { ...live, blocks } } };
    }),
  markToolAllowed: (id, server, tool) =>
    set((s) => {
      const live = s.live[id];
      if (!live) return s;
      const blocks = live.blocks.map((b) =>
        b.kind === "tool" &&
        b.call.status === "awaitingApproval" &&
        b.call.server === server &&
        b.call.tool === tool
          ? { ...b, call: { ...b.call, approval: null, status: "approved" as const } }
          : b,
      );
      return { live: { ...s.live, [id]: { ...live, blocks } } };
    }),
}));

/** Resets the store (tests). */
export function resetStreams(): void {
  useStreams.setState({ live: {}, failures: {} });
}
