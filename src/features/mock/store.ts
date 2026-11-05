import { create } from "zustand";
import { MOCK_REPLY, seedArtifacts, seedConversations, seedMessages, seedProjects } from "./data";
import type { Artifact, Conversation, Message, Project } from "./types";

/**
 * In-memory stand-in for the backend (Phase 1 only). Phase 2 replaces it with TanStack Query
 * over `oxide-store`; components read through the selectors below, so the swap is local.
 *
 * Zustand v5 note: selectors must return stable references. Select raw slices here and
 * derive filtered/sorted lists with `useMemo` in components.
 */

interface MockData {
  conversations: Conversation[];
  messages: Record<string, Message[]>;
  projects: Project[];
  artifacts: Artifact[];
  /** A fake reply in flight (drives the stop button and status line). */
  streaming: { conversationId: string; startedAt: number } | null;
}

interface MockActions {
  toggleStar: (id: string) => void;
  rename: (id: string, title: string) => void;
  remove: (id: string) => void;
  /** Creates a chat with a first user message and a fake reply. Returns the new id. */
  createConversation: (text: string, modelId: string, projectId?: string) => string;
  sendReply: (conversationId: string, text: string) => void;
  editMessage: (conversationId: string, messageId: string, text: string) => void;
  stop: () => void;
  reset: (now?: number) => void;
}

export type MockState = MockData & MockActions;

const REPLY_DELAY_MS = 1800;
let pendingReply: ReturnType<typeof setTimeout> | null = null;
let seq = 0;

function newId(prefix: string): string {
  seq += 1;
  return `${prefix}${Date.now().toString(36)}${seq}`;
}

function seed(now: number): MockData {
  return {
    conversations: seedConversations(now),
    messages: seedMessages(now),
    projects: seedProjects(now),
    artifacts: seedArtifacts(now),
    streaming: null,
  };
}

function titleFrom(text: string): string {
  const firstLine = text.trim().split("\n")[0] ?? "";
  return firstLine.length > 48 ? `${firstLine.slice(0, 47)}…` : firstLine || "New chat";
}

export const useMock = create<MockState>()((set, get) => {
  const startReply = (conversationId: string) => {
    if (pendingReply) clearTimeout(pendingReply);
    set({ streaming: { conversationId, startedAt: Date.now() } });
    pendingReply = setTimeout(() => {
      pendingReply = null;
      const reply: Message = {
        id: newId("m"),
        role: "assistant",
        createdAt: Date.now(),
        thinking: {
          seconds: 2,
          text: "Mock mode: no model is connected yet, so this reply is canned.",
        },
        blocks: MOCK_REPLY,
      };
      set((s) => ({
        streaming: null,
        messages: {
          ...s.messages,
          [conversationId]: [...(s.messages[conversationId] ?? []), reply],
        },
      }));
    }, REPLY_DELAY_MS);
  };

  const userMessage = (text: string): Message => ({
    id: newId("m"),
    role: "user",
    createdAt: Date.now(),
    blocks: [{ type: "p", text }],
  });

  return {
    ...seed(Date.now()),

    toggleStar: (id) =>
      set((s) => ({
        conversations: s.conversations.map((c) =>
          c.id === id ? { ...c, starred: !c.starred } : c,
        ),
      })),

    rename: (id, title) => {
      const t = title.trim();
      if (!t) return;
      set((s) => ({
        conversations: s.conversations.map((c) => (c.id === id ? { ...c, title: t } : c)),
      }));
    },

    remove: (id) => {
      if (get().streaming?.conversationId === id) get().stop();
      set((s) => {
        const messages = { ...s.messages };
        delete messages[id];
        return { conversations: s.conversations.filter((c) => c.id !== id), messages };
      });
    },

    createConversation: (text, modelId, projectId) => {
      const id = newId("c");
      const convo: Conversation = {
        id,
        title: titleFrom(text),
        modelId,
        starred: false,
        updatedAt: Date.now(),
        ...(projectId ? { projectId } : {}),
      };
      set((s) => ({
        conversations: [convo, ...s.conversations],
        messages: { ...s.messages, [id]: [userMessage(text)] },
      }));
      startReply(id);
      return id;
    },

    sendReply: (conversationId, text) => {
      set((s) => ({
        messages: {
          ...s.messages,
          [conversationId]: [...(s.messages[conversationId] ?? []), userMessage(text)],
        },
        conversations: s.conversations.map((c) =>
          c.id === conversationId ? { ...c, updatedAt: Date.now() } : c,
        ),
      }));
      startReply(conversationId);
    },

    editMessage: (conversationId, messageId, text) =>
      set((s) => ({
        messages: {
          ...s.messages,
          [conversationId]: (s.messages[conversationId] ?? []).map((m) =>
            m.id === messageId ? { ...m, blocks: [{ type: "p", text }] } : m,
          ),
        },
      })),

    stop: () => {
      if (pendingReply) clearTimeout(pendingReply);
      pendingReply = null;
      set({ streaming: null });
    },

    reset: (now = Date.now()) => {
      if (pendingReply) clearTimeout(pendingReply);
      pendingReply = null;
      set(seed(now));
    },
  };
});

/** Plain-text view of a message (for copy and search). */
export function messageText(m: Message): string {
  return m.blocks
    .map((b) => (b.type === "p" ? b.text : b.type === "code" ? b.code : b.items.join("\n")))
    .join("\n\n");
}
