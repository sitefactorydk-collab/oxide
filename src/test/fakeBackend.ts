import type {
  AppSettings,
  CommandError,
  Conversation,
  Message,
  PingResponse,
  Project,
  ProviderConfig,
  Result,
  SearchHit,
} from "@/bindings";
import type { commands as RealCommands } from "@/bindings";
import { seedConversations, seedMessages, seedProjects, seedProviders } from "./fixtures";

/**
 * In-memory stand-in for the Rust backend, shaped exactly like the generated `commands`
 * object. `src/test/setup.ts` mocks `@/bindings` with it for every test.
 */
interface FakeState {
  conversations: Conversation[];
  messages: Message[];
  projects: Project[];
  providers: ProviderConfig[];
  keys: Map<string, string>;
  settings: AppSettings;
}

export interface FakeSeed {
  settings?: Partial<{ name: string; theme: AppSettings["appearance"]["theme"] }>;
  empty?: boolean;
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
    },
  };
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
  };
  state.messages.push(m);
  const c = state.conversations.find((x) => x.id === conversationId);
  if (c) c.updatedAt = m.createdAt;
  return m;
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
  startConversation: (input) => {
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
    newMessage(c.id, null, input.text.trim());
    return ok(c);
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
  addUserMessage: (conversationId, parentId, text) =>
    state.conversations.some((c) => c.id === conversationId)
      ? ok(newMessage(conversationId, parentId, text.trim()))
      : fail("notFound", "conversation not found"),
  editUserMessage: (messageId, text) => {
    const original = state.messages.find((m) => m.id === messageId);
    if (!original) return fail("notFound", "message not found");
    return ok(newMessage(original.conversationId, original.parentId, text.trim()));
  },
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
