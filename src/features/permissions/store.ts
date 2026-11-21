import { create } from "zustand";
import type { PermissionMode } from "@/bindings";

interface DraftModeState {
  /** The mode picked on a new chat's composer before the chat exists (`null`: the default). */
  mode: PermissionMode | null;
  setMode: (mode: PermissionMode | null) => void;
}

/** The permission mode of the chat being started (saved with it by `startConversation`). */
export const useDraftPermissionMode = create<DraftModeState>()((set) => ({
  mode: null,
  setMode: (mode) => set({ mode }),
}));
