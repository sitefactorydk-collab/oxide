import type { ExportFormat } from "@/bindings";
import { useExportConversation } from "@/features/chats/queries";
import { errorMessage } from "@/lib/api";
import { toast } from "@/lib/toast";
import { useBranches } from "./branches";

/** Exports a chat (the branch on screen, for Markdown) and reports where it was saved. */
export function useExportChat(): (conversationId: string, format: ExportFormat) => void {
  const exporter = useExportConversation();
  return (id, format) => {
    const leafId = useBranches.getState().leaf[id] ?? null;
    exporter.mutate(
      { id, leafId, format },
      {
        onSuccess: (path) => {
          if (path) toast(`Exported to ${path}`);
        },
        onError: (e) => toast(`Couldn’t export the chat: ${errorMessage(e)}`, "error"),
      },
    );
  };
}
