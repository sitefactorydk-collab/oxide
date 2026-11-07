import { useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef } from "react";
import { useNavigate } from "react-router";
import { commands, events, type DesktopAction } from "@/bindings";
import { chatKeys } from "@/features/chats/queries";
import { useOpenSettings } from "@/features/settings/useOpenSettings";
import { useUi } from "@/stores/ui";
import type { NewChatState } from "./keys";

/**
 * Carries out what the tray, the macOS menu, quick entry and `oxide://` links ask the main
 * window to do. Actions that arrived before the window was listening (the link Oxide was
 * started with) are fetched once, after the listener is in place.
 */
export function useDesktopActions(): void {
  const navigate = useNavigate();
  const openSettings = useOpenSettings();
  const qc = useQueryClient();
  const handle = useRef<(a: DesktopAction) => void>(() => {});

  handle.current = (action) => {
    const ui = useUi.getState();
    ui.setSearchOpen(false);
    ui.setShortcutsOpen(false);
    switch (action.type) {
      case "openChat":
        // Quick entry made the chat in another window: this one's lists don't have it yet.
        void qc.invalidateQueries({ queryKey: chatKeys.all });
        navigate(`/chat/${action.conversationId}`);
        break;
      case "newChat":
        navigate("/new", { state: { draft: action.text ?? "" } satisfies NewChatState });
        break;
      case "openSettings":
        openSettings(action.section ?? undefined);
        break;
    }
  };

  useEffect(() => {
    let disposed = false;
    let stop: (() => void) | undefined;
    events.desktopRequest
      .listen(({ payload }) => handle.current(payload))
      .then((unlisten) => {
        if (disposed) unlisten();
        else stop = unlisten;
        return commands.takeDesktopActions();
      })
      // Handled even after a StrictMode remount: the backend hands them out only once.
      .then((pending) => pending.forEach((a) => handle.current(a)))
      .catch(() => {});
    return () => {
      disposed = true;
      stop?.();
    };
  }, []);
}
