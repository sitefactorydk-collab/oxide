import { useEffect } from "react";
import { useNavigate } from "react-router";
import { useOpenSettings } from "@/features/settings/useOpenSettings";
import { isMacLike } from "@/lib/platform";
import { useUi } from "@/stores/ui";
import { matchShortcut } from "./shortcuts";

/** App-wide keyboard shortcuts (list in `shortcuts.ts`). */
export function useGlobalShortcuts(): void {
  const navigate = useNavigate();
  const openSettings = useOpenSettings();

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const id = matchShortcut(e, isMacLike);
      if (!id) return;
      e.preventDefault();
      const ui = useUi.getState();
      switch (id) {
        case "search":
          ui.setShortcutsOpen(false);
          ui.setSearchOpen(!ui.searchOpen);
          break;
        case "shortcuts":
          ui.setSearchOpen(false);
          ui.setShortcutsOpen(!ui.shortcutsOpen);
          break;
        case "toggleSidebar":
          ui.toggleSidebar();
          break;
        case "newChat":
          ui.setSearchOpen(false);
          ui.setShortcutsOpen(false);
          navigate("/new");
          break;
        case "settings":
          openSettings("general");
          break;
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [navigate, openSettings]);
}
