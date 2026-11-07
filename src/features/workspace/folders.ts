import { commands } from "@/bindings";
import { baseName } from "@/features/attachments/useAttachments";
import { errorMessage, unwrap } from "@/lib/api";
import { toast } from "@/lib/toast";
import { pickFolder, useAddChatFolder, useChatWorkspace, useRemoveChatFolder } from "./queries";
import { useDraftFolders } from "./store";

export interface FolderView {
  path: string;
  name: string;
}

/**
 * The folders of the chat being written in: a saved chat's own, or (before the chat exists)
 * a draft that `useStartConversation` grants with the new chat.
 */
export function useFolderControls(conversationId: string | undefined) {
  const ws = useChatWorkspace(conversationId);
  const draft = useDraftFolders((s) => s.folders);
  const addDraft = useDraftFolders((s) => s.add);
  const removeDraft = useDraftFolders((s) => s.remove);
  const add = useAddChatFolder(conversationId ?? "");
  const remove = useRemoveChatFolder(conversationId ?? "");

  const folders: FolderView[] = conversationId
    ? (ws.data?.folders ?? []).map((f) => ({ path: f.path, name: f.name }))
    : draft.map((p) => ({ path: p, name: baseName(p) || p }));

  /** Adds a folder by path (a picked or dropped one). */
  const addPath = async (path: string) => {
    try {
      if (conversationId) {
        await add.mutateAsync(path);
      } else {
        addDraft(await unwrap(commands.resolveFolder(path)));
      }
    } catch (e) {
      toast(errorMessage(e), "error");
    }
  };

  const pick = async () => {
    try {
      const path = await pickFolder();
      if (path) await addPath(path);
    } catch (e) {
      toast(errorMessage(e), "error");
    }
  };

  const removePath = (path: string) => {
    if (conversationId) {
      remove.mutate(path, { onError: (e) => toast(errorMessage(e), "error") });
    } else {
      removeDraft(path);
    }
  };

  return { folders, pick, addPath, remove: removePath };
}
