import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";
import { safeLocalStorage } from "@/lib/storage";
import { useUi } from "@/stores/ui";

/** Folders picked on a new-chat screen, granted when the chat is created. */
interface DraftFolders {
  folders: string[];
  add: (path: string) => void;
  remove: (path: string) => void;
  clear: () => void;
}

export const useDraftFolders = create<DraftFolders>()((set) => ({
  folders: [],
  add: (path) => set((s) => (s.folders.includes(path) ? s : { folders: [...s.folders, path] })),
  remove: (path) => set((s) => ({ folders: s.folders.filter((f) => f !== path) })),
  clear: () => set({ folders: [] }),
}));

/** Most chats whose panel state is remembered. */
const MAX_REMEMBERED = 300;

/**
 * Whether the session panel is open, per chat (remembered across restarts). A chat without
 * a choice of its own follows the last choice made anywhere (`useUi.rightPanelOpen`).
 */
interface SessionPanelState {
  open: Record<string, boolean>;
  set: (conversationId: string, open: boolean) => void;
  /** Opens the panel when a chat's task list first appears, unless the user chose already. */
  autoOpen: (conversationId: string) => void;
}

export const useSessionPanelStore = create<SessionPanelState>()(
  persist(
    (set) => ({
      open: {},
      set: (id, open) => {
        useUi.setState({ rightPanelOpen: open });
        set((s) => {
          const next = { ...s.open };
          delete next[id];
          next[id] = open;
          const keys = Object.keys(next);
          for (const k of keys.slice(0, Math.max(0, keys.length - MAX_REMEMBERED))) {
            delete next[k];
          }
          return { open: next };
        });
      },
      autoOpen: (id) => set((s) => (id in s.open ? s : { open: { ...s.open, [id]: true } })),
    }),
    {
      name: "oxide-session-panel",
      version: 1,
      storage: createJSONStorage(safeLocalStorage),
    },
  ),
);

/** The panel's state in a chat, and a toggle. */
export function useSessionPanel(conversationId: string) {
  const own = useSessionPanelStore((s) => s.open[conversationId]);
  const fallback = useUi((s) => s.rightPanelOpen);
  const set = useSessionPanelStore((s) => s.set);
  const open = own ?? fallback;
  return {
    open,
    setOpen: (v: boolean) => set(conversationId, v),
    toggle: () => set(conversationId, !open),
  };
}

export function resetWorkspaceUi(): void {
  useDraftFolders.setState({ folders: [] });
  useSessionPanelStore.setState({ open: {} });
}
