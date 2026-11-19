import type {
  AppSettings,
  Artifact,
  ArtifactKind,
  ArtifactVersion,
  Attachment,
  AttachmentLimits,
  Compaction,
  ContentBlock,
  ChatStreamEvent,
  CommandError,
  Conversation,
  ConversationChanged,
  DesktopAction,
  DesktopStatus,
  GenerateOptions,
  KnowledgeFile,
  KnowledgeText,
  McpEvent,
  MemoryChanged,
  MemoryItem,
  Message,
  PingResponse,
  Project,
  ProviderConfig,
  PythonChanged,
  SandboxChange,
  ProviderModels,
  Result,
  SearchHit,
  StreamEvent,
  ApprovalDecision,
  BrowserFrame,
  BrowserStatus,
  ChatWorkspace,
  ChromeStatus,
  ComputerChanged,
  ScheduleChange,
  TaskList,
  ToolApprovalRequest,
  ToolUsage,
  UpdateStatus,
  WebSearchKeys,
} from "@/bindings";
import type { commands as RealCommands, events as RealEvents } from "@/bindings";
import { fakeBrowserCommands, resetFakeBrowser, setBrowserEmitters } from "./fakeBrowser";
import {
  configureFakeChrome,
  fakeChromeCommands,
  resetFakeChrome,
  setChromeEmitter,
} from "./fakeChrome";
import { fakeComputerCommands, resetFakeComputer, setComputerEmitter } from "./fakeComputer";
import { fakeDataCommands, resetFakeData, setUpdateEmitter } from "./fakeData";
import { fakeInsightCommands, resetFakeInsights } from "./fakeInsights";
import { fakeGenerationCommands, resetFakeGeneration } from "./fakeGeneration";
import { fakeExtensionCommands, resetFakeExtensions } from "./fakeExtensions";
import { fakeMcpCommands, fakeMcpState, resetFakeMcp, setMcpEmitter } from "./fakeMcp";
import { fakePythonCommands, resetFakePython, setPythonEmitter } from "./fakePython";
import { fakeSandboxCommands, resetFakeSandbox, setSandboxEmitter } from "./fakeSandbox";
import {
  fakeScheduleCommands,
  forgetChatRuns,
  resetFakeSchedule,
  setScheduleEmitter,
} from "./fakeSchedule";
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
  /** Knowledge files with their text. */
  knowledge: KnowledgeText[];
  memory: MemoryItem[];
  providers: ProviderConfig[];
  keys: Map<string, string>;
  settings: AppSettings;
  models: ProviderModels[];
  replies: ReplyMode;
  held: Map<string, HeldReply>;
  /** Options of every reply started, in order. */
  replyOptions: GenerateOptions[];
  /** Attachments prepared so far, by path. */
  attachments: Map<string, Attachment>;
  /** Files "saved" by exports: path → content description. */
  exports: Array<{ id: string; leafId: string | null; format: string }>;
  /** Links opened outside the app. */
  opened: string[];
  /** Tool calls waiting for approval. */
  pending: ToolApprovalRequest[];
  /** Approval answers, in order. */
  answers: Array<{ conversationId: string; toolUseId: string; decision: ApprovalDecision }>;
  toolUsage: ToolUsage[];
  grants: Map<string, string[]>;
  /** Granted folders per chat. */
  workspaces: Map<string, ChatWorkspace>;
  tasks: Map<string, TaskList>;
  /** Stored summaries ("Compact now", or `compactHeldReply`). */
  compactions: Compaction[];
  searchKeys: WebSearchKeys;
  clock: { offset: number; zone: string | null } | null;
  artifacts: Artifact[];
  artifactVersions: ArtifactVersion[];
  /** Downloads asked for: artifact id and version. */
  artifactDownloads: Array<{ id: string; version: number | null }>;
  desktop: {
    /** Actions waiting for the main window (`takeDesktopActions`). */
    pending: DesktopAction[];
    /** Times quick entry was shown / hidden, and chats it opened. */
    quickShown: number;
    quickHidden: number;
    /** Test notifications sent from Settings → Desktop. */
    testNotifications: number;
    quickChats: string[];
    screenshots: number;
    /** Makes the shortcut fail to register (as if another app had it). */
    shortcutTaken: boolean;
    screenshotFails: boolean;
  };
}

export interface FakeSeed {
  settings?: Partial<{
    name: string;
    theme: AppSettings["appearance"]["theme"];
    /** Changes to the sandbox settings (off by default). */
    sandbox: Partial<AppSettings["sandbox"]>;
    /** Changes to the browser settings (off by default). */
    browser: Partial<AppSettings["browser"]>;
    /** Changes to the Chrome settings (off by default). */
    chrome: Partial<AppSettings["chrome"]>;
    /** Changes to the model settings. */
    models: Partial<AppSettings["models"]>;
    /** Changes to the computer use settings (off by default). */
    computer: Partial<AppSettings["computer"]>;
    /** Changes to context management. */
    context: Partial<AppSettings["context"]>;
    /** Interface language. */
    language: AppSettings["appearance"]["language"];
    /** Changes to dictation (Settings → Voice). */
    dictation: Partial<AppSettings["dictation"]>;
    /** Changes to Settings → Generation. */
    generation: Partial<AppSettings["generation"]>;
  }>;
  empty?: boolean;
  replies?: ReplyMode;
  /** Desktop actions that arrived before the window was listening (e.g. a start-up link). */
  desktopActions?: DesktopAction[];
}

let state: FakeState;
let seq = 0;

export function resetFakeBackend(seed: FakeSeed = {}, now = Date.now()): void {
  state = {
    conversations: seed.empty ? [] : seedConversations(now),
    messages: seed.empty ? [] : seedMessages(now),
    projects: seed.empty ? [] : seedProjects(now),
    knowledge: [],
    memory: [],
    providers: seedProviders(now),
    keys: new Map(),
    settings: {
      profile: { name: seed.settings?.name ?? "" },
      appearance: {
        theme: seed.settings?.theme ?? "system",
        chatFont: "serif",
        language: seed.settings?.language ?? "system",
      },
      models: {
        showUnsupported: false,
        autoTitle: true,
        titleModel: null,
        thinkingBudget: 8000,
        images: [],
        ...seed.settings?.models,
      },
      tools: { autoAllowReadOnly: false, askBeforeWeb: false, maxSteps: 25 },
      webSearch: { provider: "none", searxngUrl: "", anthropicNative: true, maxResults: 5 },
      projects: { knowledgeBudget: 40000 },
      memory: { enabled: false, model: null },
      desktop: {
        closeToTray: true,
        quickEntry: true,
        quickEntryShortcut: "Alt+Space",
        launchAtLogin: false,
        notifications: true,
      },
      shell: { enabled: false, allowlist: [], timeoutSecs: 120 },
      python: { enabled: true, timeoutSecs: 120 },
      sandbox: {
        enabled: false,
        image: "python:3.12-bookworm",
        filesImage: "mcp/filesystem",
        cpus: 2,
        memoryMb: 2048,
        network: false,
        storage: "volume",
        folder: "",
        idleMinutes: 15,
        timeoutSecs: 600,
        ask: true,
        ...seed.settings?.sandbox,
      },
      browser: {
        enabled: false,
        source: "auto",
        cdpUrl: "http://127.0.0.1:9222",
        showWindow: false,
        ask: true,
        maxSteps: 25,
        taskTimeoutSecs: 600,
        idleMinutes: 15,
        agentModel: null,
        ...seed.settings?.browser,
      },
      chrome: { enabled: false, port: 47820, ask: false, ...seed.settings?.chrome },
      computer: {
        enabled: false,
        display: null,
        hideOxide: true,
        nativeAnthropic: true,
        alwaysAllowedApps: [],
        models: [],
        ...seed.settings?.computer,
      },
      context: {
        compaction: true,
        threshold: 75,
        compactionModel: null,
        anthropicClearing: true,
        windows: [],
        ...seed.settings?.context,
      },
      updates: { checkAutomatically: true },
      dictation: {
        enabled: true,
        providerId: null,
        model: "whisper-1",
        language: "",
        ...seed.settings?.dictation,
      },
      generation: {
        text: { providerId: null, baseUrl: "", model: "" },
        image: { providerId: null, baseUrl: "", model: "gpt-image-2" },
        video: { providerId: null, baseUrl: "", model: "" },
        audio: { providerId: null, baseUrl: "", model: "gpt-4o-mini-tts" },
        voice: "alloy",
        audioFormat: "mp3",
        ...seed.settings?.generation,
      },
    },
    models: seedModels(),
    replies: seed.replies ?? "auto",
    held: new Map(),
    replyOptions: [],
    attachments: new Map(),
    exports: [],
    opened: [],
    pending: [],
    answers: [],
    toolUsage: [],
    grants: new Map(),
    workspaces: new Map(),
    tasks: new Map(),
    compactions: [],
    searchKeys: { brave: null, tavily: null },
    clock: null,
    artifacts: [],
    artifactVersions: [],
    artifactDownloads: [],
    desktop: {
      pending: seed.desktopActions ?? [],
      quickShown: 0,
      quickHidden: 0,
      testNotifications: 0,
      quickChats: [],
      screenshots: 0,
      shortcutTaken: false,
      screenshotFails: false,
    },
  };
  resetFakeMcp();
  resetFakeExtensions();
  resetFakePython();
  resetFakeSandbox();
  resetFakeBrowser();
  resetFakeChrome();
  resetFakeComputer();
  resetFakeSchedule();
  resetFakeData();
  resetFakeInsights();
  resetFakeGeneration();
  configureFakeChrome(state.settings.chrome.enabled, state.settings.chrome.port);
}

export const FAKE_LIMITS: AttachmentLimits = {
  maxFiles: 20,
  maxImageBytes: 30 * 1024 * 1024,
  maxImageSide: 1568,
  maxPdfBytes: 32 * 1024 * 1024,
  maxPdfPages: 100,
  maxTextBytes: 1024 * 1024,
};

/** What the fake backend makes of a file name (only the extension matters). */
function fakeAttachment(name: string, bytes: number): Attachment | string {
  const ext = name.split(".").pop()?.toLowerCase() ?? "";
  const id = nextId("att");
  const base = {
    id,
    name,
    path: `/data/attachments/${id}.${ext || "txt"}`,
    size: bytes,
    originalSize: bytes,
    width: null,
    height: null,
    pages: null,
    thumbnail: null,
    note: null,
  };
  if (["png", "jpg", "jpeg", "gif", "webp"].includes(ext)) {
    return {
      ...base,
      kind: "image",
      mediaType: ext === "png" ? "image/png" : "image/jpeg",
      width: 640,
      height: 400,
      thumbnail: "data:image/png;base64,iVBORw0KGgo=",
    };
  }
  if (ext === "pdf") return { ...base, kind: "pdf", mediaType: "application/pdf", pages: 3 };
  if (["txt", "md", "rs", "ts", "py", "json", "csv"].includes(ext)) {
    return { ...base, kind: "text", mediaType: "text/plain" };
  }
  return `${name} isn't a file type Oxide can read. Attach images, PDFs, text or code.`;
}

function blockOf(a: Attachment): ContentBlock {
  const source = { kind: "path" as const, path: a.path };
  return a.kind === "image"
    ? { type: "image", mediaType: a.mediaType, source }
    : { type: "document", mediaType: a.mediaType, name: a.name, source };
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
const mcpChanged = fakeEvent<McpEvent>("mcp-changed");
const memoryChanged = fakeEvent<MemoryChanged>("memory-changed");
const desktopRequest = fakeEvent<DesktopAction>("desktop-request");
const taskListChanged = fakeEvent<TaskList>("task-list-changed");
const pythonChanged = fakeEvent<PythonChanged>("python-changed");
setMcpEmitter((e) => void mcpChanged.emit(e));
setPythonEmitter((s) => void pythonChanged.emit(s));
const sandboxChanged = fakeEvent<SandboxChange>("sandbox-changed");
setSandboxEmitter((c) => void sandboxChanged.emit(c));
const browserChanged = fakeEvent<BrowserStatus>("browser-changed");
const browserLive = fakeEvent<BrowserFrame>("browser-live");
setBrowserEmitters(
  (s) => void browserChanged.emit(s),
  (f) => void browserLive.emit(f),
);
const chromeChanged = fakeEvent<ChromeStatus>("chrome-changed");
setChromeEmitter((s) => void chromeChanged.emit(s));
const scheduleChanged = fakeEvent<ScheduleChange>("schedule-changed");
setScheduleEmitter((c) => void scheduleChanged.emit(c));
const computerChanged = fakeEvent<ComputerChanged>("computer-changed");
setComputerEmitter((c) => void computerChanged.emit(c));
const updateChanged = fakeEvent<UpdateStatus>("update-changed");
setUpdateEmitter((s) => void updateChanged.emit(s));

export const fakeEvents = {
  chatStream,
  conversationUpdated,
  mcpChanged,
  memoryChanged,
  desktopRequest,
  taskListChanged,
  pythonChanged,
  sandboxChanged,
  browserChanged,
  browserLive,
  chromeChanged,
  scheduleChanged,
  computerChanged,
  updateChanged,
} as unknown as typeof RealEvents;

/** Sets a chat's task list, as the `task_list` tool would, and sends the event. */
export function setTaskList(conversationId: string, items: TaskList["items"]): void {
  const list: TaskList = { conversationId, items, updatedAt: Date.now() };
  state.tasks.set(conversationId, list);
  void taskListChanged.emit(list);
}

/** A folder path as the fake backend resolves it (full paths only). */
function resolvePath(path: string): Result<string, CommandError> {
  const p = path.trim().replace(/[\\/]+$/, "");
  if (!/^([A-Za-z]:\\|\/)/.test(p)) {
    return { status: "error", error: { kind: "invalid", message: `“${path}” isn't a full path.` } };
  }
  return { status: "ok", data: p };
}

function workspaceOf(conversationId: string): ChatWorkspace {
  let ws = state.workspaces.get(conversationId);
  if (!ws) {
    ws = { folders: [], allowDelete: false };
    state.workspaces.set(conversationId, ws);
  }
  return ws;
}

function grantFolder(conversationId: string, path: string): void {
  const ws = workspaceOf(conversationId);
  if (ws.folders.some((f) => f.path === path)) return;
  const name = path.split(/[\\/]/).filter(Boolean).pop() ?? path;
  ws.folders.push({ path, name, addedAt: Date.now() });
}

/** The tray, a menu or a deep link asks the main window to do something. */
export function emitDesktopRequest(action: DesktopAction): void {
  void desktopRequest.emit(action);
}

/** Makes the desktop backend misbehave for a test. */
export function setDesktopFaults(faults: { shortcutTaken?: boolean; screenshotFails?: boolean }) {
  Object.assign(state.desktop, faults);
}

function desktopStatus(): DesktopStatus {
  const d = state.settings.desktop;
  const wanted = d.quickEntry ? d.quickEntryShortcut : null;
  const taken = wanted !== null && state.desktop.shortcutTaken;
  return {
    shortcut: {
      wanted,
      active: taken ? null : wanted,
      error: taken
        ? `${wanted} couldn't be registered. Another app may already use it: pick another shortcut.`
        : null,
    },
    launchAtLogin: d.launchAtLogin,
    launchAtLoginError: null,
    deepLinkScheme: "oxide",
    devBuild: false,
  };
}

/** Memory items learned "in the background", announced like the backend does. */
export function learnMemory(items: Array<{ text: string; projectId?: string | null }>): void {
  const now = Date.now();
  for (const i of items) {
    state.memory.push({
      id: nextId("mem"),
      projectId: i.projectId ?? null,
      text: i.text,
      conversationId: "c1",
      createdAt: now,
      updatedAt: now,
    });
  }
  void memoryChanged.emit({ added: items.length });
}

function addKnowledge(
  projectId: string,
  name: string,
  kind: KnowledgeFile["kind"],
  text: string,
): Promise<Result<KnowledgeFile, CommandError>> {
  if (!state.projects.some((p) => p.id === projectId)) return fail("notFound", "project not found");
  if (!text.trim()) return fail("invalid", `“${name}” has no text to add`);
  const file: KnowledgeFile = {
    id: nextId("kf"),
    projectId,
    name: name.trim() || "Untitled",
    kind,
    mediaType: kind === "pdf" ? "application/pdf" : "text/plain",
    chars: text.trim().length,
    tokens: Math.ceil(text.trim().length / 4),
    pages: kind === "pdf" ? 3 : null,
    note: null,
    createdAt: Date.now() + state.knowledge.length,
  };
  state.knowledge.push({ file, text: text.trim() });
  return ok(file);
}

/** Sends a stream event as the backend would. */
export function emitStream(conversationId: string, event: StreamEvent): void {
  void chatStream.emit({ conversationId, event });
}

/**
 * The held reply in `conversationId` is compacted while it runs: "compacting", then
 * "compacted" (the summary is stored, as the backend does).
 */
export function compactHeldReply(
  conversationId: string,
  summary = "## Task\nA sheet.",
  trigger: Compaction["trigger"] = "auto",
): Compaction {
  const r = state.held.get(conversationId);
  if (!r) throw new Error(`no held reply in ${conversationId}`);
  const compaction: Compaction = {
    id: nextId("k"),
    conversationId,
    messageId: r.messageId,
    afterToolUseId: null,
    summary,
    trigger,
    tokensBefore: 98_000,
    tokensAfter: 6_000,
    model: r.model,
    createdAt: Date.now(),
  };
  state.compactions.push(compaction);
  emitStream(conversationId, { type: "compacting" });
  emitStream(conversationId, { type: "compacted", compaction });
  return compaction;
}

/** The held reply in `conversationId` starts summarizing the conversation. */
export function startCompacting(conversationId: string) {
  emitStream(conversationId, { type: "compacting" });
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

function newMessage(
  conversationId: string,
  parentId: string | null,
  text: string,
  attachments: Attachment[] = [],
): Message {
  const content: ContentBlock[] = attachments.map(blockOf);
  if (text) content.push({ type: "text", text });
  const m: Message = {
    id: nextId("m"),
    conversationId,
    parentId,
    role: "user",
    content,
    model: null,
    createdAt: Date.now(),
    stopReason: null,
    usage: null,
    feedback: null,
  };
  state.messages.push(m);
  const c = state.conversations.find((x) => x.id === conversationId);
  if (c) c.updatedAt = m.createdAt;
  return m;
}

function saveReply(
  r: HeldReply,
  text: string,
  stopReason: Message["stopReason"],
  content?: ContentBlock[],
): Message {
  const m: Message = {
    id: r.messageId,
    conversationId: r.conversationId,
    parentId: r.parentId,
    role: "assistant",
    content: content ?? (text ? [{ type: "text", text }] : []),
    model: r.model,
    createdAt: Date.now(),
    stopReason,
    usage: { inputTokens: 10, outputTokens: 5, cacheReadTokens: 0, cacheWriteTokens: 0 },
    feedback: null,
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

/** Ends a held reply, saving `content` as its blocks (tool calls and all). */
export function finishReplyWith(conversationId: string, content: ContentBlock[]): Message {
  const r = state.held.get(conversationId);
  if (!r) throw new Error(`no held reply in ${conversationId}`);
  state.held.delete(conversationId);
  const m = saveReply(r, "", "endTurn", content);
  emitStream(conversationId, { type: "done", message: m });
  return m;
}

/** The held reply's id (to build tool events in tests). */
export function heldReplyId(conversationId: string): string {
  const r = state.held.get(conversationId);
  if (!r) throw new Error(`no held reply in ${conversationId}`);
  return r.messageId;
}

/** Adds a tool call waiting for approval, as the backend would, and sends its events. */
export function askApproval(request: ToolApprovalRequest): void {
  state.pending.push(request);
  emitStream(request.conversationId, { type: "toolApproval", request });
}

/** Adds entries to a chat's tool log. */
export function logToolUse(entries: ToolUsage[]): void {
  state.toolUsage.push(...entries);
}

/**
 * Saves an artifact version as the `artifact` tool would (a new artifact, or the next version
 * of `key` in the chat), and returns the `artifactRef` block its tool result carries.
 */
export function saveArtifact(input: {
  conversationId: string;
  key: string;
  kind: ArtifactKind;
  title: string;
  content: string;
  language?: string | null;
  messageId?: string | null;
}): Extract<ContentBlock, { type: "artifactRef" }> {
  const now = Date.now() + state.artifactVersions.length;
  let a = state.artifacts.find(
    (x) => x.conversationId === input.conversationId && x.key === input.key,
  );
  if (a) {
    a.version += 1;
    a.kind = input.kind;
    a.title = input.title;
    a.language = input.language ?? null;
    a.updatedAt = now;
  } else {
    a = {
      id: nextId("art"),
      conversationId: input.conversationId,
      key: input.key,
      kind: input.kind,
      title: input.title,
      language: input.language ?? null,
      version: 1,
      createdAt: now,
      updatedAt: now,
    };
    state.artifacts.push(a);
  }
  state.artifactVersions.push({
    artifactId: a.id,
    version: a.version,
    kind: input.kind,
    title: input.title,
    language: input.language ?? null,
    content: input.content,
    messageId: input.messageId ?? null,
    createdAt: now,
  });
  return { type: "artifactRef", artifactId: a.id, title: a.title, version: a.version };
}

/** Switches how later replies behave. */
export function setReplyMode(mode: ReplyMode): void {
  state.replies = mode;
}

export const fakeCommands: typeof RealCommands = {
  ...fakeMcpCommands,
  ...fakeExtensionCommands,
  ...fakePythonCommands,
  ...fakeSandboxCommands,
  ...fakeBrowserCommands,
  ...fakeChromeCommands,
  ...fakeComputerCommands,
  ...fakeDataCommands,
  ...fakeInsightCommands,
  ...fakeGenerationCommands,
  ...fakeScheduleCommands,
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
    const firstLine = input.text.trim().split("\n")[0] || (input.attachments[0]?.name ?? "");
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
    if (input.tools) fakeMcpState().chatTools.set(c.id, input.tools);
    for (const f of input.folders ?? []) {
      const r = resolvePath(f);
      if (r.status === "ok") grantFolder(c.id, r.data);
    }
    const first = newMessage(c.id, null, input.text.trim(), input.attachments);
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
    forgetChatRuns(id);
    return ok(null);
  },
  listMessages: (conversationId) =>
    ok(state.messages.filter((m) => m.conversationId === conversationId)),
  sendMessage: (conversationId, parentId, text, attachments, options) => {
    if (!state.conversations.some((c) => c.id === conversationId)) {
      return fail("notFound", "conversation not found");
    }
    if (state.held.has(conversationId)) {
      return fail("invalid", "a reply is already being written in this chat");
    }
    if (!text.trim() && attachments.length === 0) return fail("invalid", "the message is empty");
    const m = newMessage(conversationId, parentId, text.trim(), attachments);
    const result = ok(m);
    startReply(conversationId, m.id, options);
    return result;
  },
  editAndResend: (messageId, text, options) => {
    const original = state.messages.find((m) => m.id === messageId);
    if (!original) return fail("notFound", "message not found");
    const kept = original.content.filter((b) => b.type === "image" || b.type === "document");
    const m = newMessage(original.conversationId, original.parentId, text.trim());
    m.content = [...kept, ...m.content];
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
  computerCalibrate: (model: string) => {
    if (model.includes("qwen3-14b")) {
      return Promise.resolve({
        status: "error" as const,
        error: {
          kind: "invalid",
          message: "The model found only 2 of 12 markers; at least 4 are needed.",
        } as CommandError,
      });
    }
    const calibration = {
      x: { scale: 10000, offset: 2 },
      y: { scale: 10000, offset: -55 },
      markers: 12,
      shown: 12,
      errorBefore: 56,
      errorAfter: 1,
      width: 1366,
      height: 768,
      grid: false,
      at: Date.now(),
    };
    const models = state.settings.computer.models.filter((m) => m.model !== model);
    const old = state.settings.computer.models.find((m) => m.model === model);
    state.settings = {
      ...state.settings,
      computer: {
        ...state.settings.computer,
        models: [...models, { model, grid: old?.grid ?? false, calibration }],
      },
    };
    return ok(calibration);
  },
  listModels: () =>
    ok(
      state.models.map((l) => ({
        ...l,
        models: l.models.map((m) => {
          const o = state.settings.models.images.find((i) => i.model === m.id);
          return o ? { ...m, capabilities: { ...m.capabilities, images: o.images } } : m;
        }),
      })),
    ),
  chatContext: (conversationId, leafId) => {
    const convo = state.conversations.find((c) => c.id === conversationId);
    if (!convo) return fail("notFound", "conversation not found");
    const thread: Message[] = [];
    let cur = state.messages.find((m) => m.id === leafId);
    while (cur) {
      thread.unshift(cur);
      const parent: string | null = cur.parentId;
      cur = state.messages.find((m) => m.id === parent);
    }
    const chars = thread
      .flatMap((m) => m.content)
      .reduce((n, b) => n + (b.type === "text" ? b.text.length : 400), 0);
    const listed = state.models.flatMap((l) => l.models).find((m) => m.id === convo.model);
    const override = state.settings.context.windows.find((w) => w.model === convo.model);
    const window = override?.tokens ?? listed?.contextWindow ?? 32_768;
    const compacted = state.compactions.some(
      (c) => c.conversationId === conversationId && thread.some((m) => m.id === c.messageId),
    );
    return ok({
      conversationId,
      tokens: compacted ? 3_000 : 2_500 + Math.ceil(chars / 4),
      window,
      windowSource: override ? "setting" : listed?.contextWindow ? "provider" : "default",
      threshold: state.settings.context.compaction
        ? Math.floor((window * state.settings.context.threshold) / 100)
        : 0,
      compacted,
      clearedResults: 0,
    });
  },
  chatCompactions: (conversationId) =>
    ok(state.compactions.filter((c) => c.conversationId === conversationId)),
  compactChat: (conversationId, leafId) => {
    if (state.held.has(conversationId)) {
      return fail("invalid", "a reply is already being written in this chat");
    }
    if (!state.messages.some((m) => m.id === leafId)) return fail("notFound", "message not found");
    const compaction: Compaction = {
      id: nextId("k"),
      conversationId,
      messageId: leafId,
      afterToolUseId: null,
      summary: "## Task\nThe chat so far, summarized.",
      trigger: "manual",
      tokensBefore: 12_000,
      tokensAfter: 900,
      model: null,
      createdAt: Date.now(),
    };
    state.compactions.push(compaction);
    return ok(compaction);
  },
  setMessageFeedback: (messageId, feedback) => {
    const m = state.messages.find((x) => x.id === messageId);
    if (!m) return fail("notFound", "message not found");
    if (m.role !== "assistant") return fail("invalid", "only replies can be rated");
    m.feedback = feedback;
    return ok(m);
  },
  exportConversation: (id, leafId, format) => {
    if (!state.conversations.some((c) => c.id === id)) {
      return fail("notFound", "conversation not found");
    }
    state.exports.push({ id, leafId, format });
    const ext = format === "json" ? "json" : format === "html" ? "html" : "md";
    return ok(`C:\\Users\\me\\Documents\\export.${ext}`);
  },
  createDemoConversation: (model) => {
    const now = Date.now();
    const c: Conversation = {
      id: nextId("c"),
      title: "Demo: Markdown, code, math and images",
      projectId: null,
      model,
      starred: false,
      createdAt: now,
      updatedAt: now,
    };
    state.conversations.push(c);
    return ok(c);
  },
  pickFiles: () => ok<string[]>([]),
  prepareAttachment: (source) => {
    const name =
      source.kind === "path" ? (source.path.split(/[\\/]/).pop() ?? source.path) : source.name;
    const bytes = source.kind === "bytes" ? Math.floor((source.data.length * 3) / 4) : 1234;
    const a = fakeAttachment(name, bytes);
    if (typeof a === "string") return fail("invalid", a);
    state.attachments.set(a.path, a);
    return ok(a);
  },
  attachmentPreview: (path) =>
    state.attachments.has(path) || path.startsWith("/data/attachments/")
      ? ok("data:image/png;base64,iVBORw0KGgo=")
      : fail("invalid", "an attachment isn't in Oxide's attachments folder"),
  attachmentLimits: () => Promise.resolve(FAKE_LIMITS),
  openExternal: (url) => {
    state.opened.push(url);
    return ok(null);
  },
  desktopStatus: () => Promise.resolve(desktopStatus()),
  testNotification: () => {
    state.desktop.testNotifications += 1;
    return ok(null);
  },
  takeDesktopActions: () => {
    const pending = state.desktop.pending;
    state.desktop.pending = [];
    return Promise.resolve(pending);
  },
  showQuickEntry: () => {
    state.desktop.quickShown += 1;
    return Promise.resolve();
  },
  hideQuickEntry: () => {
    state.desktop.quickHidden += 1;
    return Promise.resolve();
  },
  openQuickEntryChat: (conversationId) => {
    state.desktop.quickHidden += 1;
    state.desktop.quickChats.push(conversationId);
    void desktopRequest.emit({ type: "openChat", conversationId });
    return Promise.resolve();
  },
  captureScreenshot: () => {
    if (state.desktop.screenshotFails) {
      return fail<Attachment>("invalid", "The screenshot couldn't be taken: no screen");
    }
    state.desktop.screenshots += 1;
    const path = `/data/attachments/shot-${state.desktop.screenshots}.jpg`;
    const a: Attachment = {
      id: nextId("att"),
      name: "Screenshot.png",
      kind: "image",
      mediaType: "image/jpeg",
      path,
      size: 120_000,
      originalSize: 900_000,
      width: 1568,
      height: 882,
      pages: null,
      thumbnail: "data:image/png;base64,iVBORw0KGgo=",
      note: null,
    };
    state.attachments.set(path, a);
    return ok(a);
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
      archived: false,
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
    if (patch.instructions !== null) p.instructions = patch.instructions.trim();
    if (patch.archived !== null) p.archived = patch.archived;
    p.updatedAt = Date.now();
    return ok({ ...p });
  },
  deleteProject: (id) => {
    state.projects = state.projects.filter((p) => p.id !== id);
    state.knowledge = state.knowledge.filter((k) => k.file.projectId !== id);
    state.memory = state.memory.filter((m) => m.projectId !== id);
    for (const c of state.conversations) if (c.projectId === id) c.projectId = null;
    return ok(null);
  },
  setConversationProject: (id, projectId) => {
    const c = state.conversations.find((x) => x.id === id);
    if (!c) return fail("notFound", "conversation not found");
    if (projectId !== null && !state.projects.some((p) => p.id === projectId)) {
      return fail("notFound", "project not found");
    }
    c.projectId = projectId;
    return ok({ ...c });
  },
  projectKnowledge: (projectId) => {
    const files = state.knowledge.filter((k) => k.file.projectId === projectId).map((k) => k.file);
    const totalTokens = files.reduce((n, f) => n + f.tokens, 0);
    const budgetTokens = state.settings.projects.knowledgeBudget;
    return ok({
      files,
      totalTokens,
      budgetTokens,
      mode: files.length === 0 ? "empty" : totalTokens <= budgetTokens ? "full" : "search",
    });
  },
  addProjectFile: (projectId, source) => {
    const name =
      source.kind === "path" ? (source.path.split(/[\\/]/).pop() ?? "file") : source.name;
    const ext = name.split(".").pop()?.toLowerCase() ?? "";
    if (["png", "jpg", "jpeg", "gif", "webp"].includes(ext)) {
      return fail(
        "invalid",
        `${name} is an image. Project knowledge takes text, code and PDF files.`,
      );
    }
    const text =
      source.kind === "bytes"
        ? atob(source.data)
        : `Contents of ${name}. `.repeat(ext === "pdf" ? 40 : 10);
    return addKnowledge(projectId, name, ext === "pdf" ? "pdf" : "text", text);
  },
  addProjectText: (projectId, name, text) => addKnowledge(projectId, name, "note", text),
  getKnowledgeFile: (id) => {
    const k = state.knowledge.find((x) => x.file.id === id);
    return k ? ok(k) : fail("notFound", "knowledge file not found");
  },
  deleteKnowledgeFile: (id) => {
    state.knowledge = state.knowledge.filter((k) => k.file.id !== id);
    return ok(null);
  },
  listArtifacts: () =>
    ok(
      [...state.artifacts]
        .sort((a, b) => b.updatedAt - a.updatedAt)
        .map((a) => {
          const latest = state.artifactVersions.find(
            (v) => v.artifactId === a.id && v.version === a.version,
          );
          return {
            artifact: { ...a },
            conversationTitle:
              state.conversations.find((c) => c.id === a.conversationId)?.title ?? "",
            excerpt: (latest?.content ?? "").replace(/\s+/g, " ").slice(0, 160),
          };
        }),
    ),
  conversationArtifacts: (conversationId) =>
    ok(state.artifacts.filter((a) => a.conversationId === conversationId).map((a) => ({ ...a }))),
  getArtifact: (id) => {
    const a = state.artifacts.find((x) => x.id === id);
    if (!a) return fail("notFound", "artifact not found");
    return ok({
      artifact: { ...a },
      versions: state.artifactVersions
        .filter((v) => v.artifactId === id)
        .sort((x, y) => x.version - y.version),
    });
  },
  deleteArtifact: (id) => {
    if (!state.artifacts.some((a) => a.id === id)) return fail("notFound", "artifact not found");
    state.artifacts = state.artifacts.filter((a) => a.id !== id);
    state.artifactVersions = state.artifactVersions.filter((v) => v.artifactId !== id);
    return ok(null);
  },
  saveArtifactFile: (id, version) => {
    state.artifactDownloads.push({ id, version });
    return ok(`C:\\Downloads\\${id}.txt`);
  },
  listMemory: () => ok([...state.memory]),
  addMemory: (input) => {
    const text = input.text.trim();
    if (!text) return fail("invalid", "the memory is empty");
    const now = Date.now();
    const m: MemoryItem = {
      id: nextId("mem"),
      projectId: input.projectId,
      text,
      conversationId: null,
      createdAt: now,
      updatedAt: now,
    };
    state.memory.push(m);
    return ok(m);
  },
  updateMemory: (id, text) => {
    const m = state.memory.find((x) => x.id === id);
    if (!m) return fail("notFound", "memory not found");
    if (!text.trim()) return fail("invalid", "the memory is empty");
    m.text = text.trim();
    m.updatedAt = Date.now();
    return ok({ ...m });
  },
  deleteMemory: (id) => {
    state.memory = state.memory.filter((m) => m.id !== id);
    return ok(null);
  },
  clearMemory: (projectId) => {
    const before = state.memory.length;
    state.memory = projectId === null ? [] : state.memory.filter((m) => m.projectId !== projectId);
    return ok(before - state.memory.length);
  },

  answerToolApproval: (conversationId, toolUseId, decision) => {
    const had = state.pending.some(
      (p) => p.conversationId === conversationId && p.toolUseId === toolUseId,
    );
    const req = state.pending.find(
      (p) => p.conversationId === conversationId && p.toolUseId === toolUseId,
    );
    state.pending = state.pending.filter((p) => p.toolUseId !== toolUseId);
    state.answers.push({ conversationId, toolUseId, decision });
    if (decision === "allowChat") {
      // The backend's "Allow always" keys: `server/tool`, `builtin/tool`, and one key per
      // shell command.
      let key = `fs/${toolUseId}`;
      if (req?.server) key = `${req.server}/${req.tool}`;
      else if (req?.tool === "run_command") {
        let cmd = "";
        try {
          cmd = String((JSON.parse(req.inputJson) as { command?: unknown }).command ?? "").trim();
        } catch {
          cmd = "";
        }
        key = `builtin/run_command:${cmd}`;
      } else if (req) key = `builtin/${req.tool}`;
      const g = state.grants.get(conversationId) ?? [];
      if (!g.includes(key)) state.grants.set(conversationId, [...g, key]);
    }
    return ok(had);
  },
  pendingToolApprovals: () => ok(state.pending),
  toolUsage: (conversationId) =>
    ok(state.toolUsage.filter((u) => u.conversationId === conversationId)),
  chatToolGrants: (conversationId) => ok(state.grants.get(conversationId) ?? []),
  revokeChatToolGrant: (conversationId, toolKey) => {
    const g = state.grants.get(conversationId) ?? [];
    state.grants.set(conversationId, toolKey === null ? [] : g.filter((k) => k !== toolKey));
    return ok(state.grants.get(conversationId) ?? []);
  },
  resolveFolder: (path) => Promise.resolve(resolvePath(path)),
  chatWorkspace: (conversationId) => ok(structuredClone(workspaceOf(conversationId))),
  addChatFolder: (conversationId, path) => {
    if (!state.conversations.some((c) => c.id === conversationId)) {
      return fail("notFound", "conversation not found");
    }
    const r = resolvePath(path);
    if (r.status === "error") return Promise.resolve(r);
    grantFolder(conversationId, r.data);
    return ok(structuredClone(workspaceOf(conversationId)));
  },
  removeChatFolder: (conversationId, path) => {
    const ws = workspaceOf(conversationId);
    ws.folders = ws.folders.filter((f) => f.path !== path);
    return ok(structuredClone(ws));
  },
  setChatAllowDelete: (conversationId, allow) => {
    workspaceOf(conversationId).allowDelete = allow;
    return ok(structuredClone(workspaceOf(conversationId)));
  },
  chatTaskList: (conversationId) =>
    ok(state.tasks.get(conversationId) ?? { conversationId, items: [], updatedAt: 0 }),
  openChatFolder: (_conversationId, path) => {
    state.opened.push(path);
    return ok(null);
  },
  setClientClock: (offset, zone) => {
    state.clock = { offset, zone };
    return ok(null);
  },
  webSearchKeys: () => ok(state.searchKeys),
  setWebSearchKey: (service, key) => {
    state.searchKeys = { ...state.searchKeys, [service]: key ? mask(key) : null };
    return ok(state.searchKeys);
  },
  testWebSearch: (settings, key) => {
    const stored = settings.provider === "brave" ? state.searchKeys.brave : state.searchKeys.tavily;
    if ((settings.provider === "brave" || settings.provider === "tavily") && !key && !stored) {
      return ok({ ok: false, message: "Brave Search needs an API key", sample: [] });
    }
    return ok({ ok: true, message: "2 results for a test search.", sample: ["Oxide", "Rust"] });
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
    configureFakeChrome(state.settings.chrome.enabled, state.settings.chrome.port);
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
