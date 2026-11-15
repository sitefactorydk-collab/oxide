import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";
import type { ChatFont, Effort, ThemePref, UiLanguage } from "@/bindings";
import { safeLocalStorage } from "@/lib/storage";

export type { ChatFont, ThemePref, UiLanguage };

export const SIDEBAR_MIN = 220;
export const SIDEBAR_MAX = 420;
export const SIDEBAR_DEFAULT = 280;

export function clampSidebarWidth(w: number): number {
  if (!Number.isFinite(w)) return SIDEBAR_DEFAULT;
  return Math.round(Math.min(SIDEBAR_MAX, Math.max(SIDEBAR_MIN, w)));
}

interface UiData {
  /**
   * First-paint cache of stored settings (profile name, theme, chat font). The source of
   * truth is the settings document in SQLite; `useSettingsSync` keeps these in step, and
   * persisting them here means the right theme shows before the backend answers.
   * Change them through `features/settings/queries.ts`, not directly.
   */
  userName: string;
  theme: ThemePref;
  chatFont: ChatFont;
  /** Interface language (Phase 18). */
  language: UiLanguage;
  /** Phase 1 kept the cached values above as the only copy; they're imported once. */
  settingsMigrated: boolean;
  sidebarOpen: boolean;
  sidebarWidth: number;
  rightPanelOpen: boolean;
  starredOpen: boolean;
  recentsOpen: boolean;
  /**
   * Model last picked for new chats (`<providerId>/<model>`). When it isn't available,
   * `useNewChatModel` falls back to a listed default.
   */
  modelId: string;
  thinking: boolean;
  /**
   * Effort level picked in the composer, or `null` for each model's default. Models that
   * don't offer the level use the nearest lower one (see `effectiveEffort`).
   */
  effort: Effort | null;
  /** Transient overlays: not persisted. */
  searchOpen: boolean;
  shortcutsOpen: boolean;
}

interface UiActions {
  toggleSidebar: () => void;
  setSidebarWidth: (w: number) => void;
  toggleRightPanel: () => void;
  toggleSection: (section: "starred" | "recents") => void;
  setModelId: (id: string) => void;
  toggleThinking: () => void;
  setEffort: (effort: Effort | null) => void;
  setSearchOpen: (open: boolean) => void;
  setShortcutsOpen: (open: boolean) => void;
}

export type UiState = UiData & UiActions;

export const defaultUiData: UiData = {
  userName: "",
  theme: "system",
  chatFont: "serif",
  language: "system",
  settingsMigrated: false,
  sidebarOpen: true,
  sidebarWidth: SIDEBAR_DEFAULT,
  rightPanelOpen: false,
  starredOpen: true,
  recentsOpen: true,
  modelId: "anthropic/claude-sonnet-4-5",
  thinking: false,
  effort: null,
  searchOpen: false,
  shortcutsOpen: false,
};

export const useUi = create<UiState>()(
  persist(
    (set) => ({
      ...defaultUiData,
      toggleSidebar: () => set((s) => ({ sidebarOpen: !s.sidebarOpen })),
      setSidebarWidth: (w) => set({ sidebarWidth: clampSidebarWidth(w) }),
      toggleRightPanel: () => set((s) => ({ rightPanelOpen: !s.rightPanelOpen })),
      toggleSection: (section) =>
        set((s) =>
          section === "starred" ? { starredOpen: !s.starredOpen } : { recentsOpen: !s.recentsOpen },
        ),
      setModelId: (modelId) => set({ modelId }),
      toggleThinking: () => set((s) => ({ thinking: !s.thinking })),
      setEffort: (effort) => set({ effort }),
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
        language: s.language,
        settingsMigrated: s.settingsMigrated,
        sidebarOpen: s.sidebarOpen,
        sidebarWidth: s.sidebarWidth,
        rightPanelOpen: s.rightPanelOpen,
        starredOpen: s.starredOpen,
        recentsOpen: s.recentsOpen,
        modelId: s.modelId,
        thinking: s.thinking,
        effort: s.effort,
      }),
    },
  ),
);
