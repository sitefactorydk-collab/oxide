import { useMutation, useQuery, useQueryClient, type QueryClient } from "@tanstack/react-query";
import { commands, type AppDraft } from "@/bindings";
import { appKeys } from "@/features/apps/queries";
import { unwrap } from "@/lib/api";

export const builderKeys = {
  all: ["builder"] as const,
  draft: (id: string) => [...builderKeys.all, "draft", id] as const,
  checkpoints: (id: string) => [...builderKeys.all, "checkpoints", id] as const,
  chat: (cid: string) => [...builderKeys.all, "chat", cid] as const,
  drafts: () => [...builderKeys.all, "drafts"] as const,
};

export function useDraft(id: string) {
  return useQuery({
    queryKey: builderKeys.draft(id),
    queryFn: () => unwrap(commands.appDraft(id)),
    enabled: id !== "",
    staleTime: 0,
  });
}

export function useCheckpoints(id: string) {
  return useQuery({
    queryKey: builderKeys.checkpoints(id),
    queryFn: () => unwrap(commands.appCheckpoints(id)),
    enabled: id !== "",
  });
}

/** The builder's chat (the chat list leaves it out). */
export function useBuilderChat(cid: string | undefined) {
  return useQuery({
    queryKey: builderKeys.chat(cid ?? ""),
    queryFn: () => unwrap(commands.getConversation(cid ?? "")),
    enabled: Boolean(cid),
  });
}

export function useAppDrafts() {
  return useQuery({
    queryKey: builderKeys.drafts(),
    queryFn: () => unwrap(commands.listAppDrafts()),
  });
}

/** Puts a draft the backend returned into the cache (Preview and Code follow it). */
export function setDraft(qc: QueryClient, draft: AppDraft) {
  qc.setQueryData(builderKeys.draft(draft.app.manifest.id), draft);
  void qc.invalidateQueries({ queryKey: builderKeys.checkpoints(draft.app.manifest.id) });
}

export function useOpenBuilder() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (v: { id: string | null; model: string }) =>
      unwrap(commands.openAppBuilder(v.id, v.model)),
    onSuccess: (draft) => {
      setDraft(qc, draft);
      void qc.invalidateQueries({ queryKey: builderKeys.drafts() });
    },
  });
}

function useDraftMutation<V>(run: (v: V) => Promise<AppDraft>) {
  const qc = useQueryClient();
  return useMutation({ mutationFn: run, onSuccess: (draft) => setDraft(qc, draft) });
}

export function useSaveDraftFile(id: string) {
  return useDraftMutation((v: { path: string; content: string }) =>
    unwrap(commands.saveDraftFile(id, v.path, v.content)),
  );
}

export function useDeleteDraftFile(id: string) {
  return useDraftMutation((path: string) => unwrap(commands.deleteDraftFile(id, path)));
}

export function useRenameDraftFile(id: string) {
  return useDraftMutation((v: { from: string; to: string }) =>
    unwrap(commands.renameDraftFile(id, v.from, v.to)),
  );
}

export function useUpdateDetails(id: string) {
  return useDraftMutation(
    (v: { name: string; description: string; icon: string; category: string }) =>
      unwrap(commands.updateDraftDetails(id, v.name, v.description, v.icon, v.category)),
  );
}

export function useRestoreCheckpoint(id: string) {
  return useDraftMutation((checkpointId: string) =>
    unwrap(commands.restoreAppCheckpoint(id, checkpointId)),
  );
}

export function usePublishDraft(id: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => unwrap(commands.publishAppDraft(id)),
    onSuccess: () =>
      Promise.all([
        qc.invalidateQueries({ queryKey: builderKeys.draft(id) }),
        qc.invalidateQueries({ queryKey: builderKeys.drafts() }),
        qc.invalidateQueries({ queryKey: appKeys.all }),
      ]),
  });
}

export function useDiscardDraft() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => unwrap(commands.discardAppDraft(id)),
    onSuccess: (_, id) => {
      qc.removeQueries({ queryKey: builderKeys.draft(id) });
      return Promise.all([
        qc.invalidateQueries({ queryKey: builderKeys.drafts() }),
        qc.invalidateQueries({ queryKey: appKeys.all }),
      ]);
    },
  });
}

/** The text sent by **Fix with AI**. */
export function fixMessage(error: string): string {
  return `The preview shows this error:\n\n\`\`\`\n${error.trim()}\n\`\`\`\n\nPlease fix it.`;
}
