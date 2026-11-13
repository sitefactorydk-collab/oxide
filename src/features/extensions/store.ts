import { create } from "zustand";

/** A `.mcpb` dropped on the window (or opened elsewhere) waiting for Settings → Extensions. */
interface ExtensionDropState {
  pending: string | null;
  request: (path: string) => void;
  take: () => string | null;
}

export const useExtensionDrop = create<ExtensionDropState>()((set, get) => ({
  pending: null,
  request: (path) => set({ pending: path }),
  take: () => {
    const p = get().pending;
    if (p !== null) set({ pending: null });
    return p;
  },
}));

/** Desktop extension bundles, by file name. */
export function isExtensionFile(path: string): boolean {
  return /\.(mcpb|dxt)$/i.test(path);
}

export function resetExtensionDrop(): void {
  useExtensionDrop.setState({ pending: null });
}

/** A skill to open in Settings → Skills (a chat's "Open in Settings" after `save_skill`). */
interface SkillFocusState {
  folder: string | null;
  focus: (folder: string) => void;
  clear: () => void;
}

export const useSkillFocus = create<SkillFocusState>()((set) => ({
  folder: null,
  focus: (folder) => set({ folder }),
  clear: () => set({ folder: null }),
}));
