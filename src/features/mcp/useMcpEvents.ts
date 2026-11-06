import { useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { commands, events } from "@/bindings";
import { appendLogLine, applyServerState, mcpKeys, removeServerState } from "./queries";
import { useMcpUi } from "./store";

/**
 * App-wide listener for `mcpChanged`: server states and log lines go into the query cache,
 * progress and server questions into `useMcpUi`. Mounted once in `AppLayout`.
 */
export function useMcpEvents(): void {
  const qc = useQueryClient();
  useEffect(() => {
    let disposed = false;
    let stop: (() => void) | undefined;
    events.mcpChanged
      .listen(({ payload: e }) => {
        const ui = useMcpUi.getState();
        switch (e.type) {
          case "server":
            applyServerState(qc, e.state);
            if (e.state.status !== "running") {
              void qc.invalidateQueries({ queryKey: mcpKeys.lists(e.state.name) });
            }
            break;
          case "removed":
            removeServerState(qc, e.name);
            break;
          case "lists":
            void qc.invalidateQueries({ queryKey: mcpKeys.lists(e.server) });
            break;
          case "log":
            appendLogLine(qc, e.server, e.line);
            break;
          case "progress":
            ui.setProgress(e.token, {
              progress: e.progress,
              total: e.total,
              message: e.message,
            });
            break;
          case "ask":
            ui.addAsk(e.interaction);
            break;
          case "askClosed":
            ui.closeAsk(e.id);
            break;
        }
      })
      .then((unlisten) => (disposed ? unlisten() : (stop = unlisten)))
      .catch(() => {});
    // Questions asked before this window loaded.
    void commands
      .mcpPendingAsks()
      .then((asks) => {
        if (!disposed) useMcpUi.getState().setAsks(asks);
      })
      .catch(() => {});
    return () => {
      disposed = true;
      stop?.();
    };
  }, [qc]);
}
