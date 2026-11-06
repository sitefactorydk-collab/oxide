import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  commands,
  type ApprovalDecision,
  type SearchKeyKind,
  type WebSearchSettings,
} from "@/bindings";
import { unwrap } from "@/lib/api";

export const toolKeys = {
  usage: (conversationId: string) => ["tools", "usage", conversationId] as const,
  grants: (conversationId: string) => ["tools", "grants", conversationId] as const,
  pending: ["tools", "pending"] as const,
  searchKeys: ["tools", "searchKeys"] as const,
};

/** Every tool call of a chat ("Used in this chat"). */
export function useToolUsage(conversationId: string) {
  return useQuery({
    queryKey: toolKeys.usage(conversationId),
    queryFn: () => unwrap(commands.toolUsage(conversationId)),
    enabled: conversationId !== "",
  });
}

/** Tools allowed for the rest of a chat. */
export function useChatGrants(conversationId: string) {
  return useQuery({
    queryKey: toolKeys.grants(conversationId),
    queryFn: () => unwrap(commands.chatToolGrants(conversationId)),
    enabled: conversationId !== "",
  });
}

export function useRevokeGrant(conversationId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (toolKey: string | null) =>
      unwrap(commands.revokeChatToolGrant(conversationId, toolKey)),
    onSuccess: (grants) => qc.setQueryData(toolKeys.grants(conversationId), grants),
  });
}

/** Tool calls waiting for approval (for chats whose live reply this window didn't see). */
export function usePendingApprovals(enabled: boolean) {
  return useQuery({
    queryKey: toolKeys.pending,
    queryFn: () => unwrap(commands.pendingToolApprovals()),
    enabled,
  });
}

export function useAnswerApproval() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (v: { conversationId: string; toolUseId: string; decision: ApprovalDecision }) =>
      unwrap(commands.answerToolApproval(v.conversationId, v.toolUseId, v.decision)),
    onSettled: (_r, _e, v) => {
      void qc.invalidateQueries({ queryKey: toolKeys.pending });
      if (v.decision === "allowChat") {
        void qc.invalidateQueries({ queryKey: toolKeys.grants(v.conversationId) });
      }
    },
  });
}

export function useSearchKeys() {
  return useQuery({
    queryKey: toolKeys.searchKeys,
    queryFn: () => unwrap(commands.webSearchKeys()),
  });
}

export function useSetSearchKey() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (v: { service: SearchKeyKind; key: string | null }) =>
      unwrap(commands.setWebSearchKey(v.service, v.key)),
    onSuccess: (keys) => qc.setQueryData(toolKeys.searchKeys, keys),
  });
}

export function useTestSearch() {
  return useMutation({
    mutationFn: (v: { settings: WebSearchSettings; key: string | null }) =>
      unwrap(commands.testWebSearch(v.settings, v.key)),
  });
}
