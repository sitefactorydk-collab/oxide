import type {
  AppSettings,
  ChatStreamEvent,
  CommandError,
  Conversation,
  ConversationChanged,
  GenerateOptions,
  Message,
  PingResponse,
  Project,
  ProviderConfig,
  ProviderModels,
  Result,
  SearchHit,
  StreamEvent,
} from "@/bindings";
import type { commands as RealCommands, events as RealEvents } from "@/bindings";
import {
  seedConversations,
  seedMessages,
  seedModels,
  seedProjects,
  seedProviders,
} from "./fixtures";

/**
 * In-memory stand-in for the Rust backend, shaped exactly like the generated `commands`
 * and `events` objects. `src/test/setup.ts` mocks `@/bindings` with them for every test.
 *
 * Replies: every send answers with "Reply to: <text>" through `chatStream` events, like the
 * real backend. `replies: "hold"` keeps a reply streaming until `finishReply()` or Stop;
 * `replies: "fail"` makes them fail.
 */
export type ReplyMode = "auto" | "hold" | "fail";

interface HeldReply {
  conversationId: string;
  messageId: string;
  parentId: string;
  model: string;
  text: string;
}

interface FakeState {
  conversations: Conversation[];
  messages: Message[];
  projects: Project[];
  providers: ProviderConfig[];
  keys: Map<string, string>;
  settings: AppSettings;
  models: ProviderModels[];
  replies: ReplyMode;
  held: Map<string, HeldReply>;
  /** Options of every reply started, in order. */
  replyOptions: GenerateOptions[];
}

export interface FakeSeed {
  settings?: Partial<{ name: string; theme: AppSettings["appearance"]["theme"] }>;
  empty?: boolean;
  replies?: ReplyMode;
}

let state: FakeState;
let seq = 0;

export function resetFakeBackend(seed: FakeSeed = {}, now = Date.now()): void {
  state = {
    conversations: seed.empty ? [] : seedConversations(now),
    messages: seed.empty ? [] : seedMessages(now),
    projects: seed.empty ? [] : seedProjects(now),
    providers: seedProviders(now),
    keys: new Map(),
    settings: {
      profile: { name: seed.settings?.name ?? "" },
      appearance: { theme: seed.settings?.theme ?? "system", chatFont: "serif" },
      models: { showUnsupported: false, autoTitle: true, titleModel: null, thinkingBudget: 8000 },
    },
    models: seedModels(),
    replies: seed.replies ?? "auto",
    held: new Map(),
    replyOptions: [],
  };
}

/** A tiny typed event bus with the shape of the generated `events` object. */
function fakeEvent<T>(name: string) {
  const listeners = new Set<(e: { event: string; id: number; payload: T }) => void>();
  return {
    listen: (cb: (e: { event: string; id: number; payload: T }) => void) => {
      listeners.add(cb);
      return Promise.resolve(() => {
        listeners.delete(cb);
      });
    },
    once: (cb: (e: { event: string; id: number; payload: T }) => void) => {
      const wrapped = (e: { event: string; id: number; payload: T }) => {
        listeners.delete(wrapped);
        cb(e);
      };
      listeners.add(wrapped);
      return Promise.resolve(() => {
        listeners.delete(wrapped);
      });
    },
    emit: (payload: T) => {
      for (const cb of [...listeners]) cb({ event: name, id: 0, payload });
      return Promise.resolve();
    },
  };
}

const chatStream = fakeEvent<ChatStreamEvent>("chat-stream");
const conversationUpdated = fakeEvent<ConversationChanged>("conversation-updated");

export const fakeEvents = { chatStream, conversationUpdated } as unknown as typeof RealEvents;

/** Sends a stream event as the backend would. */
export function emitStream(conversationId: string, event: StreamEvent): void {
  void chatStream.emit({ conversationId, event });
}

/** Sends a conversation update as the backend would (e.g. an automatic title). */
export function emitConversationUpdated(conversation: Conversation): void {
  void conversationUpdated.emit({ conversation });
}

resetFakeBackend();

/** Read access for assertions. */
export function fakeState(): Readonly<FakeState> {
  return state;
}

const ok = <T>(data: T): Promise<Result<T, CommandError>> =>
  Promise.resolve({ status: "ok", data: JSON.parse(JSON.stringify(data)) as T });
const fail = <T>(kind: CommandError["kind"], message: string): Promise<Result<T, CommandError>> =>
  Promise.resolve({ status: "error", error: { kind, message } as CommandError });

function nextId(prefix: string): string {
  seq += 1;
  return `${prefix}-${seq}`;
}

function sorted(list: Conversation[]): Conversation[] {
  return [...list].sort((a, b) => b.updatedAt - a.updatedAt);
}

function textOf(m: Message): string {
  return m.content.flatMap((b) => (b.type === "text" ? [b.text] : [])).join("\n\n");
}

function withConversation<T>(
  id: string,
  fn: (c: Conversation) => T,
): Promise<Result<T, CommandError>> {
  const c = state.conversations.find((x) => x.id === id);
  return c ? ok(fn(c)) : fail("notFound", "conversation not found");
}

function mask(key: string): string {
  return key.length < 12 ? "••••" : `••••${key.slice(-4)}`;
}

function newMessage(conversationId: string, parentId: string | null, text: string): Message {
  const m: Message = {
    id: nextId("m"),
    conversationId,
    parentId,
    role: "user",
    content: [{ type: "text", text }],
    model: null,
    createdAt: Date.now(),
    stopReason: null,
    usage: null,
  };
  state.messages.push(m);
  const c = state.conversations.find((x) => x.id === conversationId);
  if (c) c.updatedAt = m.createdAt;
  return m;
}

function saveReply(r: HeldReply, text: string, stopReason: Message["stopReason"]): Message {
  const m: Message = {
    id: r.messageId,
    conversationId: r.conversationId,
    parentId: r.parentId,
    role: "assistant",
    content: text ? [{ type: "text", text }] : [],
    model: r.model,
    createdAt: Date.now(),
    stopReason,
    usage: { inputTokens: 10, outputTokens: 5, cacheReadTokens: 0, cacheWriteTokens: 0 },
  };
  state.messages.push(m);
  const c = state.conversations.find((x) => x.id === r.conversationId);
  if (c) c.updatedAt = m.createdAt;
  return m;
}

/**
 * Starts a reply to `parentId` the way the backend does: `started` at once, then deltas and
 * `done` (or `failed`) shortly after.
 */
function startReply(conversationId: string, parentId: string, options: GenerateOptions): string {
  const c = state.conversations.find((x) => x.id === conversationId);
  const question = state.messages.find((m) => m.id === parentId);
  const r: HeldReply = {
    conversationId,
    messageId: nextId("r"),
    parentId,
    model: c?.model ?? "",
    text: `Reply to: ${question ? textOf(question) : ""}`,
  };
  state.replyOptions.push(options);
  emitStream(conversationId, {
    type: "started",
    messageId: r.messageId,
    parentId,
    model: r.model,
    startedAt: Date.now(),
  });
  const mode = state.replies;
  setTimeout(() => {
    if (mode === "fail") {
      emitStream(conversationId, {
        type: "failed",
        error: "LM Studio: couldn't reach http://127.0.0.1:1234/v1",
        message: null,
      });
      return;
    }
    if (mode === "hold") {
      state.held.set(conversationId, r);
      emitStream(conversationId, { type: "textDelta", text: "Partial " });
      return;
    }
    emitStream(conversationId, { type: "textDelta", text: r.text });
    emitStream(conversationId, { type: "done", message: saveReply(r, r.text, "endTurn") });
  }, 0);
  return r.messageId;
}

/** Ends a held reply normally. */
export function finishReply(conversationId: string): void {
  const r = state.held.get(conversationId);
  if (!r) throw new Error(`no held reply in ${conversationId}`);
  state.held.delete(conversationId);
  emitStream(conversationId, { type: "textDelta", text: r.text });
  emitStream(conversationId, {
    type: "done",
    message: saveReply(r, `Partial ${r.text}`, "endTurn"),
  });
}

/** Switches how later replies behave. */
export function setReplyMode(mode: ReplyMode): void {
  state.replies = mode;
}

export const fakeCommands: typeof RealCommands = {
  ping: (name: string): Promise<PingResponse> =>
    Promise.resolve({
      message: `pong, ${name}`,
      appName: "Oxide",
      appVersion: "0.1.0",
      coreVersion: "0.1.0",
    }),

  listConversations: () => ok(sorted(state.conversations)),
  getConversation: (id) => withConversation(id, (c) => c),
  startConversation: (input, options) => {
    const now = Date.now();
    const firstLine = input.text.trim().split("\n")[0] ?? "";
    const c: Conversation = {
      id: nextId("c"),
      title: firstLine.length > 48 ? `${firstLine.slice(0, 47)}…` : firstLine || "New chat",
      projectId: input.projectId,
      model: input.model,
      starred: false,
      createdAt: now,
      updatedAt: now,
    };
    state.conversations.push(c);
    const first = newMessage(c.id, null, input.text.trim());
    const result = ok(c);
    startReply(c.id, first.id, options);
    return result;
  },
  renameConversation: (id, title) =>
    title.trim()
      ? withConversation(id, (c) => {
          c.title = title.trim();
          return c;
        })
      : fail("invalid", "title is empty"),
  setConversationStarred: (id, starred) =>
    withConversation(id, (c) => {
      c.starred = starred;
      return c;
    }),
  setConversationModel: (id, model) =>
    withConversation(id, (c) => {
      c.model = model;
      return c;
    }),
  deleteConversation: (id) => {
    if (!state.conversations.some((c) => c.id === id)) {
      return fail("notFound", "conversation not found");
    }
    state.conversations = state.conversations.filter((c) => c.id !== id);
    state.messages = state.messages.filter((m) => m.conversationId !== id);
    return ok(null);
  },
  listMessages: (conversationId) =>
    ok(state.messages.filter((m) => m.conversationId === conversationId)),
  sendMessage: (conversationId, parentId, text, options) => {
    if (!state.conversations.some((c) => c.id === conversationId)) {
      return fail("notFound", "conversation not found");
    }
    if (state.held.has(conversationId)) {
      return fail("invalid", "a reply is already being written in this chat");
    }
    const m = newMessage(conversationId, parentId, text.trim());
    const result = ok(m);
    startReply(conversationId, m.id, options);
    return result;
  },
  editAndResend: (messageId, text, options) => {
    const original = state.messages.find((m) => m.id === messageId);
    if (!original) return fail("notFound", "message not found");
    const m = newMessage(original.conversationId, original.parentId, text.trim());
    const result = ok(m);
    startReply(m.conversationId, m.id, options);
    return result;
  },
  regenerate: (messageId, options) => {
    const m = state.messages.find((x) => x.id === messageId);
    if (!m) return fail("notFound", "message not found");
    const userId = m.role === "user" ? m.id : m.parentId;
    if (!userId) return fail("invalid", "this reply has no question");
    return ok(startReply(m.conversationId, userId, options));
  },
  stopGeneration: (conversationId) => {
    const r = state.held.get(conversationId);
    if (!r) return ok(false);
    state.held.delete(conversationId);
    const message = saveReply(r, "Partial", "stopped");
    setTimeout(() => emitStream(conversationId, { type: "done", message }), 0);
    return ok(true);
  },
  listModels: () => ok(state.models),
  searchConversations: (query, limit) => {
    const needle = query.trim().toLowerCase();
    if (!needle) return ok<SearchHit[]>([]);
    const hits: SearchHit[] = [];
    for (const c of sorted(state.conversations)) {
      if (c.title.toLowerCase().includes(needle)) {
        hits.push({ conversation: c, messageId: null, snippet: null });
        continue;
      }
      for (const m of state.messages.filter((x) => x.conversationId === c.id)) {
        const text = textOf(m);
        const at = text.toLowerCase().indexOf(needle);
        if (at >= 0) {
          const end = at + needle.length;
          const snippet = `${text.slice(Math.max(0, at - 30), at)}\u0002${text.slice(at, end)}\u0003${text.slice(end, end + 50)}`;
          hits.push({ conversation: c, messageId: m.id, snippet });
          break;
        }
      }
    }
    return ok(hits.slice(0, limit));
  },

  listProjects: () => ok([...state.projects].sort((a, b) => b.updatedAt - a.updatedAt)),
  getProject: (id) => {
    const p = state.projects.find((x) => x.id === id);
    return p ? ok(p) : fail("notFound", "project not found");
  },
  createProject: (input) => {
    if (!input.name.trim()) return fail("invalid", "project name is empty");
    const now = Date.now();
    const p: Project = {
      id: nextId("p"),
      name: input.name.trim(),
      description: input.description.trim(),
      instructions: "",
      createdAt: now,
      updatedAt: now,
    };
    state.projects.push(p);
    return ok(p);
  },
  updateProject: (id, patch) => {
    const p = state.projects.find((x) => x.id === id);
    if (!p) return fail("notFound", "project not found");
    if (patch.name !== null) p.name = patch.name;
    if (patch.description !== null) p.description = patch.description;
    if (patch.instructions !== null) p.instructions = patch.instructions;
    p.updatedAt = Date.now();
    return ok(p);
  },
  deleteProject: (id) => {
    state.projects = state.projects.filter((p) => p.id !== id);
    for (const c of state.conversations) if (c.projectId === id) c.projectId = null;
    return ok(null);
  },

  getSettings: () => ok(state.settings),
  saveSettings: (settings) => {
    state.settings = {
      ...settings,
      profile: { name: settings.profile.name.trim() },
      models: {
        ...settings.models,
        thinkingBudget: Math.min(64000, Math.max(1024, settings.models.thinkingBudget)),
      },
    };
    return ok(state.settings);
  },

  listProviders: () => ok(state.providers),
  saveProvider: (input) => {
    const now = Date.now();
    if (!/^https?:\/\//.test(input.baseUrl.trim())) {
      return fail("invalid", "the base URL must start with http:// or https://");
    }
    const baseUrl = input.baseUrl.trim().replace(/\/+$/, "");
    if (input.id) {
      const p = state.providers.find((x) => x.id === input.id);
      if (!p) return fail("notFound", "provider not found");
      Object.assign(p, { ...input, id: p.id, baseUrl, updatedAt: now });
      return ok(p);
    }
    const p: ProviderConfig = {
      ...input,
      id: nextId("prov"),
      baseUrl,
      keyHint: null,
      createdAt: now,
      updatedAt: now,
    };
    state.providers.push(p);
    return ok(p);
  },
  deleteProvider: (id) => {
    state.providers = state.providers.filter((p) => p.id !== id);
    state.keys.delete(id);
    return ok(null);
  },
  setProviderKey: (id, key) => {
    const p = state.providers.find((x) => x.id === id);
    if (!p) return fail("notFound", "provider not found");
    if (!key.trim()) return fail("invalid", "the API key is empty");
    state.keys.set(id, key.trim());
    p.keyHint = mask(key.trim());
    return ok(p);
  },
  clearProviderKey: (id) => {
    const p = state.providers.find((x) => x.id === id);
    if (!p) return fail("notFound", "provider not found");
    state.keys.delete(id);
    p.keyHint = null;
    return ok(p);
  },
  testProvider: (input) => {
    const key = input.apiKey ?? (input.providerId ? state.keys.get(input.providerId) : undefined);
    const needsKey = input.kind === "anthropic" || input.kind === "openai";
    if (needsKey && !key) {
      return ok({
        ok: false,
        message: "Anthropic needs an API key.",
        modelCount: 0,
        models: [],
        latencyMs: 3,
      });
    }
    return ok({
      ok: true,
      message: "Connected. 2 models available.",
      modelCount: 2,
      models: ["model-a", "model-b"],
      latencyMs: 3,
    });
  },
};
