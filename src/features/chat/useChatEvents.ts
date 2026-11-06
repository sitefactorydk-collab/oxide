import { useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { commands, events } from "@/bindings";
import { toolKeys } from "@/features/tools/queries";
import { addMessageToCache, chatKeys, replaceConversation } from "@/features/chats/queries";
import { useStreams } from "./streams";

/**
 * Listens to reply events for every chat, app-wide, so a reply keeps streaming into the
 * store while the user looks at another page. Saved replies go into the query cache.
 */
export function useChatEvents(): void {
  const qc = useQueryClient();
  // Models are told the user's local date and time.
  useEffect(() => {
    const sync = () => {
      let zone: string | null = null;
      try {
        zone = Intl.DateTimeFormat().resolvedOptions().timeZone ?? null;
      } catch {
        zone = null;
      }
      void commands.setClientClock(-new Date().getTimezoneOffset(), zone).catch(() => {});
    };
    sync();
    window.addEventListener("focus", sync);
    return () => window.removeEventListener("focus", sync);
  }, []);

  useEffect(() => {
    let disposed = false;
    const stops: Array<() => void> = [];
    const keep = (pending: Promise<() => void>) => {
      pending.then((unlisten) => (disposed ? unlisten() : stops.push(unlisten))).catch(() => {});
    };
    keep(
      events.chatStream.listen(({ payload }) => {
        useStreams.getState().apply(payload);
        const e = payload.event;
        if (e.type === "toolResult") {
          void qc.invalidateQueries({ queryKey: toolKeys.usage(payload.conversationId) });
        }
        if (e.type === "toolApproval" || e.type === "done" || e.type === "failed") {
          void qc.invalidateQueries({ queryKey: toolKeys.pending });
        }
        if (e.type === "done" || e.type === "failed") {
          if (e.message) addMessageToCache(qc, e.message);
          void qc.invalidateQueries({ queryKey: chatKeys.messages(payload.conversationId) });
          void qc.invalidateQueries({ queryKey: [...chatKeys.all, "search"] });
        }
      }),
    );
    keep(
      events.conversationUpdated.listen(({ payload }) => {
        replaceConversation(qc, payload.conversation);
        void qc.invalidateQueries({ queryKey: [...chatKeys.all, "search"] });
      }),
    );
    return () => {
      disposed = true;
      stops.forEach((stop) => stop());
    };
  }, [qc]);
}
