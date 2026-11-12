import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { commands, type Compaction } from "@/bindings";
import { unwrap } from "@/lib/api";

export const contextKeys = {
  all: ["context"] as const,
  chat: (conversationId: string) => ["context", conversationId] as const,
  usage: (conversationId: string, leafId: string | null, modelId: string) =>
    ["context", conversationId, "usage", leafId, modelId] as const,
  compactions: (conversationId: string) => ["context", conversationId, "compactions"] as const,
};

const NO_COMPACTIONS: Compaction[] = [];

/** How full a chat's context is for the thread ending at `leafId`. */
export function useChatContext(conversationId: string, leafId: string | null, modelId: string) {
  return useQuery({
    queryKey: contextKeys.usage(conversationId, leafId, modelId),
    queryFn: () => unwrap(commands.chatContext(conversationId, leafId)),
    enabled: conversationId !== "",
    staleTime: 30_000,
  });
}

/** The summaries that stand in for the start of a chat. */
export function useCompactions(conversationId: string) {
  const q = useQuery({
    queryKey: contextKeys.compactions(conversationId),
    queryFn: () => unwrap(commands.chatCompactions(conversationId)),
    enabled: conversationId !== "",
  });
  return q.data ?? NO_COMPACTIONS;
}

/** **Compact now**. */
export function useCompactNow() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ conversationId, leafId }: { conversationId: string; leafId: string }) =>
      unwrap(commands.compactChat(conversationId, leafId)),
    onSettled: (_data, _error, { conversationId }) =>
      qc.invalidateQueries({ queryKey: contextKeys.chat(conversationId) }),
  });
}

const compact = new Intl.NumberFormat("en-US", { notation: "compact", maximumFractionDigits: 1 });

/** 131072 → "131K". */
export function tokenCount(n: number): string {
  return compact.format(n);
}
