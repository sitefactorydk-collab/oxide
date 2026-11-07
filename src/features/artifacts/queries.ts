import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { commands } from "@/bindings";
import { unwrap } from "@/lib/api";

export const artifactKeys = {
  all: ["artifacts"] as const,
  list: ["artifacts", "list"] as const,
  chat: (conversationId: string) => ["artifacts", "chat", conversationId] as const,
  detail: (id: string) => ["artifacts", "detail", id] as const,
};

/** Every artifact (the gallery). */
export function useArtifacts() {
  return useQuery({
    queryKey: artifactKeys.list,
    queryFn: () => unwrap(commands.listArtifacts()),
  });
}

/** A chat's artifacts. */
export function useChatArtifacts(conversationId: string) {
  return useQuery({
    queryKey: artifactKeys.chat(conversationId),
    queryFn: () => unwrap(commands.conversationArtifacts(conversationId)),
    enabled: conversationId !== "",
  });
}

/** An artifact with its versions. */
export function useArtifact(id: string | null) {
  return useQuery({
    queryKey: artifactKeys.detail(id ?? ""),
    queryFn: () => unwrap(commands.getArtifact(id ?? "")),
    enabled: !!id,
  });
}

export function useDeleteArtifact() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => unwrap(commands.deleteArtifact(id)),
    onSuccess: () => qc.invalidateQueries({ queryKey: artifactKeys.all }),
  });
}

/** Opens the save dialog for a version. Resolves to the saved path, or null if cancelled. */
export function useSaveArtifactFile() {
  return useMutation({
    mutationFn: ({ id, version }: { id: string; version: number | null }) =>
      unwrap(commands.saveArtifactFile(id, version)),
  });
}
