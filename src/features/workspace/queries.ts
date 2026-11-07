import { useMutation, useQuery, useQueryClient, type QueryClient } from "@tanstack/react-query";
import { commands, type ChatWorkspace, type TaskList } from "@/bindings";
import { unwrap } from "@/lib/api";

export const workspaceKeys = {
  all: ["workspace"] as const,
  folders: (conversationId: string) => ["workspace", "folders", conversationId] as const,
  tasks: (conversationId: string) => ["workspace", "tasks", conversationId] as const,
};

/** A chat's granted folders and whether it may delete for good. */
export function useChatWorkspace(conversationId: string | undefined) {
  return useQuery({
    queryKey: workspaceKeys.folders(conversationId ?? ""),
    queryFn: () => unwrap(commands.chatWorkspace(conversationId ?? "")),
    enabled: !!conversationId,
  });
}

/** A chat's task list ("Progress"). Updated live by `taskListChanged` events. */
export function useTaskList(conversationId: string | undefined) {
  return useQuery({
    queryKey: workspaceKeys.tasks(conversationId ?? ""),
    queryFn: () => unwrap(commands.chatTaskList(conversationId ?? "")),
    enabled: !!conversationId,
  });
}

function useWorkspaceMutation<V>(run: (v: V) => Promise<ChatWorkspace>, id: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: run,
    onSuccess: (ws) => qc.setQueryData(workspaceKeys.folders(id), ws),
  });
}

export function useAddChatFolder(conversationId: string) {
  return useWorkspaceMutation(
    (path: string) => unwrap(commands.addChatFolder(conversationId, path)),
    conversationId,
  );
}

export function useRemoveChatFolder(conversationId: string) {
  return useWorkspaceMutation(
    (path: string) => unwrap(commands.removeChatFolder(conversationId, path)),
    conversationId,
  );
}

export function useSetAllowDelete(conversationId: string) {
  return useWorkspaceMutation(
    (allow: boolean) => unwrap(commands.setChatAllowDelete(conversationId, allow)),
    conversationId,
  );
}

/** Opens the system folder picker. Returns the chosen folder, or `null` when cancelled. */
export async function pickFolder(): Promise<string | null> {
  const picked = await unwrap(commands.pickPaths("folder", false));
  return picked[0] ?? null;
}

/** Puts a task list that arrived as an event into the cache. */
export function setTaskListData(qc: QueryClient, list: TaskList): void {
  qc.setQueryData(workspaceKeys.tasks(list.conversationId), list);
}
