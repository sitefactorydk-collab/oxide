import { useMutation, useQuery, useQueryClient, type QueryClient } from "@tanstack/react-query";
import {
  commands,
  type ChatTools,
  type McpLogLine,
  type McpServerConfig,
  type McpServerLists,
  type McpServerState,
} from "@/bindings";
import { unwrap } from "@/lib/api";

export const mcpKeys = {
  all: ["mcp"] as const,
  servers: ["mcp", "servers"] as const,
  lists: (name: string) => ["mcp", "lists", name] as const,
  logs: (name: string) => ["mcp", "logs", name] as const,
  serverConfig: (name: string) => ["mcp", "serverConfig", name] as const,
  config: ["mcp", "config"] as const,
  importPreview: ["mcp", "importPreview"] as const,
  chatTools: (id: string) => ["mcp", "chatTools", id] as const,
};

const NO_SERVERS: McpServerState[] = [];
const NO_LISTS: McpServerLists = { tools: [], resources: [], templates: [], prompts: [] };
const NO_LOGS: McpLogLine[] = [];
/** Lines kept per server in the cache (the backend keeps the same number). */
const MAX_LOG_LINES = 1000;

/** Every configured server, kept current by `mcpChanged` events. */
export function useMcpServers() {
  const q = useQuery({ queryKey: mcpKeys.servers, queryFn: () => commands.mcpServers() });
  return { ...q, data: q.data ?? NO_SERVERS };
}

export function useMcpLists(name: string, enabled = true) {
  const q = useQuery({
    queryKey: mcpKeys.lists(name),
    queryFn: () => commands.mcpServerLists(name),
    enabled,
  });
  return { ...q, data: q.data ?? NO_LISTS };
}

export function useMcpLogs(name: string) {
  const q = useQuery({ queryKey: mcpKeys.logs(name), queryFn: () => commands.mcpLogs(name) });
  return { ...q, data: q.data ?? NO_LOGS };
}

export function useMcpServerConfig(name: string | null) {
  return useQuery({
    queryKey: mcpKeys.serverConfig(name ?? ""),
    queryFn: () => unwrap(commands.mcpServerConfig(name ?? "")),
    enabled: name !== null,
    staleTime: 0,
  });
}

export function useMcpConfig() {
  return useQuery({
    queryKey: mcpKeys.config,
    queryFn: () => unwrap(commands.mcpConfig()),
    staleTime: 0,
  });
}

export function useImportPreview(enabled: boolean) {
  return useQuery({
    queryKey: mcpKeys.importPreview,
    queryFn: () => unwrap(commands.mcpImportPreview()),
    enabled,
    staleTime: 0,
  });
}

/** Mutations that change the config: the servers list refreshes from events, but refetch
 * anyway so the UI never waits on one. */
function useConfigMutation<V, R>(run: (v: V) => Promise<R>) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: run,
    onSettled: () =>
      Promise.all([
        qc.invalidateQueries({ queryKey: mcpKeys.servers }),
        qc.invalidateQueries({ queryKey: mcpKeys.config }),
        qc.invalidateQueries({ queryKey: mcpKeys.importPreview }),
      ]),
  });
}

export function useSaveMcpServer() {
  return useConfigMutation(
    ({ server, oldName }: { server: McpServerConfig; oldName: string | null }) =>
      unwrap(commands.mcpSaveServer(server, oldName)),
  );
}

export function useRemoveMcpServer() {
  return useConfigMutation((name: string) => unwrap(commands.mcpRemoveServer(name)));
}

export function useSetMcpEnabled() {
  return useConfigMutation(({ name, enabled }: { name: string; enabled: boolean }) =>
    unwrap(commands.mcpSetEnabled(name, enabled)),
  );
}

export function useRestartMcpServer() {
  return useConfigMutation((name: string) => unwrap(commands.mcpRestart(name)));
}

export function useSaveMcpConfig() {
  return useConfigMutation((text: string) => unwrap(commands.mcpSaveConfig(text)));
}

export function useImportMcp() {
  return useConfigMutation((names: string[]) => unwrap(commands.mcpImport(names)));
}

export function useMcpSignIn() {
  return useConfigMutation((name: string) => unwrap(commands.mcpSignIn(name)));
}

export function useMcpSignOut() {
  return useConfigMutation((name: string) => unwrap(commands.mcpSignOut(name)));
}

/** Puts a server's new state into the cache (from an event). */
export function applyServerState(qc: QueryClient, s: McpServerState): void {
  qc.setQueryData<McpServerState[]>(mcpKeys.servers, (prev) => {
    if (!prev) return prev;
    const i = prev.findIndex((p) => p.name === s.name);
    if (i < 0) return [...prev, s];
    const next = prev.slice();
    next[i] = s;
    return next;
  });
}

export function removeServerState(qc: QueryClient, name: string): void {
  qc.setQueryData<McpServerState[]>(mcpKeys.servers, (prev) =>
    prev?.filter((p) => p.name !== name),
  );
  qc.removeQueries({ queryKey: mcpKeys.lists(name) });
  qc.removeQueries({ queryKey: mcpKeys.logs(name) });
}

/** Appends a log line to a server's cached log (if it's being shown). */
export function appendLogLine(qc: QueryClient, server: string, line: McpLogLine): void {
  qc.setQueryData<McpLogLine[]>(mcpKeys.logs(server), (prev) => {
    if (!prev) return prev;
    const next = prev.length >= MAX_LOG_LINES ? prev.slice(1) : prev.slice();
    next.push(line);
    return next;
  });
}

// --- Per-chat tool switches -------------------------------------------------------------

export const NO_CHAT_TOOLS: Required<ChatTools> = {
  disabledServers: [],
  disabledTools: [],
  disabledBuiltins: [],
};

export function normalizeChatTools(t: ChatTools | undefined): Required<ChatTools> {
  return {
    disabledServers: t?.disabledServers ?? [],
    disabledTools: t?.disabledTools ?? [],
    disabledBuiltins: t?.disabledBuiltins ?? [],
  };
}

export function useChatTools(conversationId: string | undefined) {
  const q = useQuery({
    queryKey: mcpKeys.chatTools(conversationId ?? ""),
    queryFn: () => unwrap(commands.getChatTools(conversationId ?? "")),
    enabled: conversationId !== undefined,
  });
  return { ...q, data: normalizeChatTools(q.data) };
}

export function useSetChatTools() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, tools }: { id: string; tools: ChatTools }) =>
      unwrap(commands.setChatTools(id, tools)),
    onMutate: ({ id, tools }) => {
      const key = mcpKeys.chatTools(id);
      const before = qc.getQueryData<ChatTools>(key);
      qc.setQueryData(key, tools);
      return { before, key };
    },
    onError: (_e, _v, ctx) => {
      if (ctx) qc.setQueryData(ctx.key, ctx.before);
    },
  });
}
