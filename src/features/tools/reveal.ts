import { create } from "zustand";

/**
 * "Jump to the tool call" (Phase 24): a call can sit inside a folded group, so the group
 * hears the request here, opens, and the row is scrolled to once it's drawn.
 */
interface RevealState {
  id: string | null;
  /** Counts requests, so jumping to the same call twice opens it again. */
  nonce: number;
  reveal: (id: string) => void;
}

export const useToolReveal = create<RevealState>()((set) => ({
  id: null,
  nonce: 0,
  reveal: (id) => set((s) => ({ id, nonce: s.nonce + 1 })),
}));

/** Opens the group holding the call (if any) and scrolls to the call. */
export function jumpToToolCall(id: string): void {
  useToolReveal.getState().reveal(id);
  const scroll = (tries: number) => {
    const el = document.querySelector(`[data-tool-call="${CSS.escape(id)}"]`);
    if (el) el.scrollIntoView({ behavior: "smooth", block: "center" });
    else if (tries > 0) setTimeout(() => scroll(tries - 1), 16);
  };
  scroll(10);
}
