import { create } from "zustand";
import type { ChatStreamEvent, Message, Usage } from "@/bindings";

/** A reply being written, built from `chatStream` events. */
export interface LiveReply {
  messageId: string;
  /** The user message being answered. */
  parentId: string;
  model: string;
  startedAt: number;
  text: string;
  thinking: string;
  /** When thinking started and ended (client clock), for the "Thought for Ns" line. */
  thinkingStartedAt: number | null;
  thinkingEndedAt: number | null;
  tools: Array<{ id: string; name: string }>;
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
}

type Maps = Pick<StreamsState, "live" | "failures">;

function without<T>(map: Record<string, T>, key: string): Record<string, T> {
  if (!(key in map)) return map;
  const next = { ...map };
  delete next[key];
  return next;
}

/** Applies one stream event. Pure, so it can be tested on its own. */
export function reduceStream(
  state: Maps,
  { conversationId: id, event }: ChatStreamEvent,
  now: number,
): Maps {
  const live = state.live[id];
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
            text: "",
            thinking: "",
            thinkingStartedAt: null,
            thinkingEndedAt: null,
            tools: [],
            usage: null,
          },
        },
        failures: without(state.failures, id),
      };
    case "thinkingDelta":
      if (!live) return state;
      return {
        ...state,
        live: {
          ...state.live,
          [id]: {
            ...live,
            thinking: live.thinking + event.text,
            thinkingStartedAt: live.thinkingStartedAt ?? now,
          },
        },
      };
    case "textDelta":
      if (!live) return state;
      return {
        ...state,
        live: {
          ...state.live,
          [id]: {
            ...live,
            text: live.text + event.text,
            thinkingEndedAt:
              live.thinkingStartedAt !== null && live.thinkingEndedAt === null
                ? now
                : live.thinkingEndedAt,
          },
        },
      };
    case "toolUse":
      if (!live) return state;
      return {
        ...state,
        live: {
          ...state.live,
          [id]: { ...live, tools: [...live.tools, { id: event.id, name: event.name }] },
        },
      };
    case "usage":
      if (!live) return state;
      return { ...state, live: { ...state.live, [id]: { ...live, usage: event.usage } } };
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
}));

/** Resets the store (tests). */
export function resetStreams(): void {
  useStreams.setState({ live: {}, failures: {} });
}
