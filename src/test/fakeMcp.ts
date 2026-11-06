import type {
  Attachment,
  ChatTools,
  CommandError,
  ImportPreview,
  McpEvent,
  McpInteraction,
  McpLogLine,
  McpServerConfig,
  McpServerLists,
  McpServerState,
  Result,
  commands as RealCommands,
} from "@/bindings";

/**
 * The MCP part of the fake backend: a few servers with tools, resources and prompts, an
 * editable config, a Claude desktop config to import, logs and per-chat tool switches.
 */
interface FakeMcp {
  configs: McpServerConfig[];
  states: Map<string, McpServerState>;
  lists: Map<string, McpServerLists>;
  logs: Map<string, McpLogLine[]>;
  chatTools: Map<string, ChatTools>;
  asks: McpInteraction[];
  answers: Array<{ id: string; answer: unknown }>;
  calls: Array<{ server: string; tool: string; arguments: string; token: string | null }>;
  configText: string;
  imported: string[];
}

let mcp: FakeMcp;
let emit: (e: McpEvent) => void = () => {};

export function setMcpEmitter(fn: (e: McpEvent) => void): void {
  emit = fn;
}

function state(cfg: McpServerConfig, overrides: Partial<McpServerState> = {}): McpServerState {
  return {
    name: cfg.name,
    transport: cfg.transport,
    summary: cfg.transport === "http" ? cfg.url : [cfg.command, ...cfg.args].join(" "),
    status: cfg.enabled ? "running" : "disabled",
    error: null,
    era: cfg.enabled ? "modern" : null,
    protocolVersion: cfg.enabled ? "2026-07-28" : null,
    serverName: cfg.enabled ? `${cfg.name}-server` : null,
    serverVersion: cfg.enabled ? "1.0.0" : null,
    instructions: null,
    capabilities: {
      tools: true,
      resources: true,
      prompts: true,
      logging: false,
      listChanged: false,
    },
    tools: 0,
    resources: 0,
    prompts: 0,
    restarts: 0,
    signedIn: false,
    updatedAt: 1,
    ...overrides,
  };
}

const EVERYTHING: McpServerLists = {
  tools: [
    {
      server: "everything",
      name: "echo",
      title: null,
      description: "Echoes back the input",
      inputSchema: JSON.stringify({
        type: "object",
        properties: { message: { type: "string" } },
        required: ["message"],
      }),
      annotations: {
        title: null,
        readOnlyHint: true,
        destructiveHint: null,
        idempotentHint: null,
        openWorldHint: null,
      },
    },
    {
      server: "everything",
      name: "add",
      title: null,
      description: "Adds two numbers",
      inputSchema: JSON.stringify({
        type: "object",
        properties: { a: { type: "number" }, b: { type: "number" } },
      }),
      annotations: {
        title: null,
        readOnlyHint: null,
        destructiveHint: null,
        idempotentHint: null,
        openWorldHint: null,
      },
    },
  ],
  resources: [
    {
      server: "everything",
      uri: "test://static/resource/1",
      name: "Resource 1",
      title: null,
      description: null,
      mimeType: "text/plain",
      size: null,
    },
  ],
  templates: [],
  prompts: [
    {
      server: "everything",
      name: "simple_prompt",
      title: "Simple prompt",
      description: "A prompt without arguments",
      arguments: [],
    },
    {
      server: "everything",
      name: "complex_prompt",
      title: null,
      description: null,
      arguments: [{ name: "temperature", title: null, description: null, required: true }],
    },
  ],
};

export function resetFakeMcp(): void {
  const configs: McpServerConfig[] = [
    {
      name: "everything",
      transport: "stdio",
      command: "npx",
      args: ["-y", "@modelcontextprotocol/server-everything"],
      env: {},
      url: "",
      headers: {},
      enabled: true,
    },
    {
      name: "docs",
      transport: "http",
      command: "",
      args: [],
      env: {},
      url: "https://docs.example.com/mcp",
      headers: {},
      enabled: true,
    },
  ];
  const lists = new Map<string, McpServerLists>([["everything", EVERYTHING]]);
  const states = new Map<string, McpServerState>([
    ["everything", state(configs[0]!, { tools: 2, resources: 1, prompts: 2 })],
    [
      "docs",
      state(configs[1]!, {
        status: "needsAuth",
        error: "Sign in to connect.",
        era: null,
        protocolVersion: null,
        serverName: null,
        serverVersion: null,
      }),
    ],
  ]);
  mcp = {
    configs,
    states,
    lists,
    logs: new Map([
      [
        "everything",
        [
          { at: 1, source: "oxide", level: null, text: "started npx (pid 42)" },
          { at: 2, source: "stderr", level: null, text: "Starting default (STDIO) server..." },
        ],
      ],
    ]),
    chatTools: new Map(),
    asks: [],
    answers: [],
    calls: [],
    configText: '{\n  "mcpServers": {}\n}\n',
    imported: [],
  };
}

resetFakeMcp();

export function fakeMcpState(): Readonly<FakeMcp> {
  return mcp;
}

/** A server asks the user something (as `oxide-mcp` would through `mcpChanged`). */
export function fakeMcpAsk(interaction: McpInteraction): void {
  mcp.asks.push(interaction);
  emit({ type: "ask", interaction });
}

export function fakeMcpEmit(e: McpEvent): void {
  emit(e);
}

const ok = <T>(data: T): Promise<Result<T, CommandError>> =>
  Promise.resolve({ status: "ok", data: JSON.parse(JSON.stringify(data)) as T });
const fail = <T>(kind: CommandError["kind"], message: string): Promise<Result<T, CommandError>> =>
  Promise.resolve({ status: "error", error: { kind, message } as CommandError });

function ordered(): McpServerState[] {
  return mcp.configs.flatMap((c) => {
    const s = mcp.states.get(c.name);
    return s ? [s] : [];
  });
}

function setState(s: McpServerState): void {
  mcp.states.set(s.name, s);
  emit({ type: "server", state: s });
}

function fakeAttachment(name: string, text: string): Attachment {
  return {
    id: `mcp-${name}`,
    name,
    kind: "text",
    mediaType: "text/markdown",
    path: `/data/attachments/${name}`,
    size: text.length,
    originalSize: text.length,
    width: null,
    height: null,
    pages: null,
    thumbnail: null,
    note: null,
  } as Attachment;
}

type C = typeof RealCommands;

export const fakeMcpCommands: Pick<
  C,
  | "mcpServers"
  | "mcpServerLists"
  | "mcpLogs"
  | "mcpServerConfig"
  | "mcpSaveServer"
  | "mcpRemoveServer"
  | "mcpSetEnabled"
  | "mcpRestart"
  | "mcpConfig"
  | "mcpSaveConfig"
  | "mcpOpenConfig"
  | "mcpImportPreview"
  | "mcpImport"
  | "mcpCallTool"
  | "mcpReadResource"
  | "mcpGetPrompt"
  | "mcpAttachPrompt"
  | "mcpAttachResource"
  | "mcpPendingAsks"
  | "mcpAnswer"
  | "mcpSignIn"
  | "mcpSignOut"
  | "getChatTools"
  | "setChatTools"
> = {
  mcpServers: () => Promise.resolve(JSON.parse(JSON.stringify(ordered())) as McpServerState[]),
  mcpServerLists: (name) =>
    Promise.resolve(
      JSON.parse(
        JSON.stringify(
          mcp.lists.get(name) ?? { tools: [], resources: [], templates: [], prompts: [] },
        ),
      ) as McpServerLists,
    ),
  mcpLogs: (name) => Promise.resolve([...(mcp.logs.get(name) ?? [])]),
  mcpServerConfig: (name) => {
    const c = mcp.configs.find((x) => x.name === name);
    return c ? ok(c) : fail("invalid", `There's no server named "${name}".`);
  },
  mcpSaveServer: (server, oldName) => {
    if (!oldName && mcp.configs.some((c) => c.name === server.name)) {
      return fail("invalid", `A server named "${server.name}" already exists.`);
    }
    const i = mcp.configs.findIndex((c) => c.name === (oldName ?? server.name));
    if (i >= 0) mcp.configs[i] = server;
    else mcp.configs.push(server);
    if (oldName && oldName !== server.name) {
      mcp.states.delete(oldName);
      emit({ type: "removed", name: oldName });
    }
    setState(state(server, { status: server.enabled ? "starting" : "disabled" }));
    return ok(null);
  },
  mcpRemoveServer: (name) => {
    mcp.configs = mcp.configs.filter((c) => c.name !== name);
    mcp.states.delete(name);
    emit({ type: "removed", name });
    return ok(null);
  },
  mcpSetEnabled: (name, enabled) => {
    const c = mcp.configs.find((x) => x.name === name);
    if (!c) return fail("invalid", `There's no server named "${name}".`);
    c.enabled = enabled;
    const prev = mcp.states.get(name);
    setState(
      enabled
        ? state(c, {
            tools: prev?.tools ?? 0,
            resources: prev?.resources ?? 0,
            prompts: prev?.prompts ?? 0,
          })
        : state(c, { status: "disabled", era: null, protocolVersion: null }),
    );
    return ok(null);
  },
  mcpRestart: (name) => {
    const s = mcp.states.get(name);
    if (!s) return fail("invalid", `There's no server named "${name}".`);
    setState({ ...s, restarts: s.restarts });
    return ok(null);
  },
  mcpConfig: () =>
    ok({ file: { path: "/config/oxide_config.json", text: mcp.configText }, error: null }),
  mcpSaveConfig: (text) => {
    try {
      JSON.parse(text);
    } catch {
      return fail("invalid", "The config isn't valid JSON.");
    }
    mcp.configText = text;
    return ok(null);
  },
  mcpOpenConfig: () => ok(null),
  mcpImportPreview: () => {
    const preview: ImportPreview = {
      path: "C:\\Users\\me\\AppData\\Roaming\\Claude\\claude_desktop_config.json",
      found: true,
      candidates: [
        {
          status: "new",
          config: {
            name: "filesystem",
            transport: "stdio",
            command: "npx",
            args: ["-y", "@modelcontextprotocol/server-filesystem", "C:\\Users\\me"],
            env: {},
            url: "",
            headers: {},
            enabled: true,
          },
        },
        { status: "same", config: mcp.configs[0]! },
      ],
    };
    return ok(preview);
  },
  mcpImport: (names) => {
    mcp.imported.push(...names);
    return ok(names.length);
  },
  mcpCallTool: (server, tool, args, token) => {
    mcp.calls.push({ server, tool, arguments: args, token });
    if (!mcp.states.get(server) || mcp.states.get(server)?.status !== "running") {
      return fail("invalid", `${server} isn't running.`);
    }
    let parsed: Record<string, unknown>;
    try {
      parsed = JSON.parse(args || "{}") as Record<string, unknown>;
    } catch {
      return fail("invalid", "The arguments aren't valid JSON.");
    }
    if (token) emit({ type: "progress", token, progress: 1, total: 2, message: "Halfway" });
    const text =
      tool === "echo"
        ? `Echo: ${String(parsed.message ?? "")}`
        : tool === "add"
          ? `The sum is ${Number(parsed.a ?? 0) + Number(parsed.b ?? 0)}.`
          : "done";
    return ok({
      content: [{ type: "text", text }],
      structured: null,
      isError: false,
      durationMs: 12,
    });
  },
  mcpReadResource: (_server, uri) =>
    ok({
      contents: [
        { type: "resource", uri, mimeType: "text/plain", text: "Static text", blob: null },
      ],
    }),
  mcpGetPrompt: (_server, name) =>
    ok({
      description: null,
      messages: [{ role: "user", content: { type: "text", text: `Prompt ${name}` } }],
    }),
  mcpAttachPrompt: (_server, name, args) =>
    ok(fakeAttachment(`${name}.md`, `Prompt ${name} ${JSON.stringify(args)}`)),
  mcpAttachResource: () => ok([fakeAttachment("Resource 1.txt", "Static text")]),
  mcpPendingAsks: () => Promise.resolve([...mcp.asks]),
  mcpAnswer: (id, answer) => {
    mcp.answers.push({ id, answer });
    const had = mcp.asks.some((a) => a.id === id);
    mcp.asks = mcp.asks.filter((a) => a.id !== id);
    return Promise.resolve(had);
  },
  mcpSignIn: (name) => {
    const s = mcp.states.get(name);
    if (!s) return fail("invalid", `There's no server named "${name}".`);
    setState({
      ...s,
      status: "running",
      error: null,
      signedIn: true,
      era: "modern",
      protocolVersion: "2026-07-28",
    });
    return ok(null);
  },
  mcpSignOut: (name) => {
    const s = mcp.states.get(name);
    if (!s) return fail("invalid", `There's no server named "${name}".`);
    setState({ ...s, status: "needsAuth", signedIn: false });
    return ok(null);
  },
  getChatTools: (id) => ok(mcp.chatTools.get(id) ?? { disabledServers: [], disabledTools: [] }),
  setChatTools: (id, tools) => {
    mcp.chatTools.set(id, tools);
    return ok(null);
  },
};
