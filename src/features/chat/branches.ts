import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";
import { safeLocalStorage } from "@/lib/storage";

/** Chats whose chosen branch is remembered (oldest choices are forgotten first). */
const MAX_REMEMBERED = 200;

interface BranchState {
  /** The leaf of the branch picked with the ‹ n/m › navigator, by conversation id. */
  leaf: Record<string, string>;
  setLeaf: (conversationId: string, leafId: string) => void;
  /** Back to the newest branch (after sending, editing or regenerating). */
  clear: (conversationId: string) => void;
}

export const useBranches = create<BranchState>()(
  persist(
    (set) => ({
      leaf: {},
      setLeaf: (cid, leafId) =>
        set((s) => {
          const next = { ...s.leaf };
          delete next[cid];
          next[cid] = leafId;
          const keys = Object.keys(next);
          for (const k of keys.slice(0, Math.max(0, keys.length - MAX_REMEMBERED))) delete next[k];
          return { leaf: next };
        }),
      clear: (cid) =>
        set((s) => {
          if (!(cid in s.leaf)) return s;
          const next = { ...s.leaf };
          delete next[cid];
          return { leaf: next };
        }),
    }),
    {
      name: "oxide-branches",
      version: 1,
      storage: createJSONStorage(safeLocalStorage),
      partialize: (s) => ({ leaf: s.leaf }),
    },
  ),
);
