import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";
import { safeLocalStorage } from "@/lib/storage";

export type ThemePref = "system" | "light" | "dark";
export type ChatFont = "serif" | "sans" | "system";

export const SIDEBAR_MIN = 220;
export const SIDEBAR_MAX = 420;
export const SIDEBAR_DEFAULT = 280;

export function clampSidebarWidth(w: number): number {
  if (!Number.isFinite(w)) return SIDEBAR_DEFAULT;
  return Math.round(Math.min(SIDEBAR_MAX, Math.max(SIDEBAR_MIN, w)));
}

interface UiData {
  /** Profile name for the greeting. Phase 2 moves this into stored settings. */
  userName: string;
  theme: ThemePref;
  chatFont: ChatFont;
  sidebarOpen: boolean;
  sidebarWidth: number;
  rightPanelOpen: boolean;
  starredOpen: boolean;
  recentsOpen: boolean;
  /** Selected model id for new chats (mock list until Phase 3). */
  modelId: string;
  thinking: boolean;
  /** Transient overlays: not persisted. */
  searchOpen: boolean;
  shortcutsOpen: boolean;
}

interface UiActions {
  setUserName: (name: string) => void;
  setTheme: (theme: ThemePref) => void;
  setChatFont: (font: ChatFont) => void;
  toggleSidebar: () => void;
  setSidebarWidth: (w: number) => void;
  toggleRightPanel: () => void;
  toggleSection: (section: "starred" | "recents") => void;
  setModelId: (id: string) => void;
  toggleThinking: () => void;
  setSearchOpen: (open: boolean) => void;
  setShortcutsOpen: (open: boolean) => void;
}

export type UiState = UiData & UiActions;

export const defaultUiData: UiData = {
  userName: "",
  theme: "system",
  chatFont: "serif",
  sidebarOpen: true,
  sidebarWidth: SIDEBAR_DEFAULT,
  rightPanelOpen: false,
  starredOpen: true,
  recentsOpen: true,
  modelId: "anthropic/sonnet",
  thinking: false,
  searchOpen: false,
  shortcutsOpen: false,
};

export const useUi = create<UiState>()(
  persist(
    (set) => ({
      ...defaultUiData,
      setUserName: (userName) => set({ userName }),
      setTheme: (theme) => set({ theme }),
      setChatFont: (chatFont) => set({ chatFont }),
      toggleSidebar: () => set((s) => ({ sidebarOpen: !s.sidebarOpen })),
      setSidebarWidth: (w) => set({ sidebarWidth: clampSidebarWidth(w) }),
      toggleRightPanel: () => set((s) => ({ rightPanelOpen: !s.rightPanelOpen })),
      toggleSection: (section) =>
        set((s) =>
          section === "starred" ? { starredOpen: !s.starredOpen } : { recentsOpen: !s.recentsOpen },
        ),
      setModelId: (modelId) => set({ modelId }),
      toggleThinking: () => set((s) => ({ thinking: !s.thinking })),
      setSearchOpen: (searchOpen) => set({ searchOpen }),
      setShortcutsOpen: (shortcutsOpen) => set({ shortcutsOpen }),
    }),
    {
      name: "oxide-ui",
      version: 1,
      storage: createJSONStorage(safeLocalStorage),
      // Persist plain preferences only; overlays always start closed.
      partialize: (s): Omit<UiData, "searchOpen" | "shortcutsOpen"> => ({
        userName: s.userName,
        theme: s.theme,
        chatFont: s.chatFont,
        sidebarOpen: s.sidebarOpen,
        sidebarWidth: s.sidebarWidth,
        rightPanelOpen: s.rightPanelOpen,
        starredOpen: s.starredOpen,
        recentsOpen: s.recentsOpen,
        modelId: s.modelId,
        thinking: s.thinking,
      }),
    },
  ),
);
