import { type QueryClient, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { commands, type PermissionMode } from "@/bindings";
import { DEFAULT_SETTINGS, useSettings } from "@/features/settings/queries";
import { unwrap } from "@/lib/api";
import { useDraftPermissionMode } from "./store";

export const permissionKeys = {
  mode: (conversationId: string) => ["permissions", "mode", conversationId] as const,
};

/** A chat's permission mode (its own, else the default from Settings → Permissions). */
export function useChatPermissionMode(conversationId: string | undefined) {
  return useQuery({
    queryKey: permissionKeys.mode(conversationId ?? ""),
    queryFn: () => unwrap(commands.chatPermissionMode(conversationId ?? "")),
    enabled: !!conversationId,
  });
}

export function useSetChatPermissionMode() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (v: { conversationId: string; mode: PermissionMode }) =>
      unwrap(commands.setChatPermissionMode(v.conversationId, v.mode)),
    onMutate: (v) => {
      const key = permissionKeys.mode(v.conversationId);
      const before = qc.getQueryData<PermissionMode>(key);
      qc.setQueryData(key, v.mode);
      return { before };
    },
    onError: (_e, v, ctx) => {
      if (ctx?.before) qc.setQueryData(permissionKeys.mode(v.conversationId), ctx.before);
    },
  });
}

/**
 * The mode a composer shows and changes: a saved chat's own, or (before the chat exists) the
 * draft, which falls back to the default from Settings → Permissions.
 */
export function usePermissionMode(conversationId: string | undefined): {
  mode: PermissionMode;
  setMode: (mode: PermissionMode) => void;
} {
  const saved = useChatPermissionMode(conversationId);
  const set = useSetChatPermissionMode();
  const draft = useDraftPermissionMode((s) => s.mode);
  const setDraft = useDraftPermissionMode((s) => s.setMode);
  const { data: settings } = useSettings();
  const fallback = (settings ?? DEFAULT_SETTINGS).tools.defaultMode;
  if (conversationId) {
    return {
      mode: saved.data ?? fallback,
      setMode: (mode) => set.mutate({ conversationId, mode }),
    };
  }
  return { mode: draft ?? fallback, setMode: setDraft };
}

/**
 * A chat was just created with the draft mode (`startConversation` saved it before the first
 * reply): shows it as the chat's own and clears the draft, so the next new chat starts from
 * the default again.
 */
export function applyDraftPermissionMode(qc: QueryClient, conversationId: string): void {
  const { mode, setMode } = useDraftPermissionMode.getState();
  if (mode === null) return;
  qc.setQueryData(permissionKeys.mode(conversationId), mode);
  setMode(null);
}
