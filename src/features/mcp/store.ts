import { create } from "zustand";
import type { ChatTools, McpInteraction } from "@/bindings";

export interface CallProgress {
  progress: number;
  total: number | null;
  message: string | null;
}

/**
 * Live MCP state that isn't backend data: questions servers are waiting on, progress of
 * running calls (by the token the UI chose), and the tool switches of a chat that doesn't
 * exist yet (applied once it's created).
 */
interface McpUiState {
  asks: McpInteraction[];
  progress: Record<string, CallProgress>;
  draftTools: Required<ChatTools>;
  addAsk: (i: McpInteraction) => void;
  closeAsk: (id: string) => void;
  setAsks: (all: McpInteraction[]) => void;
  setProgress: (token: string, p: CallProgress) => void;
  clearProgress: (token: string) => void;
  setDraftTools: (t: Required<ChatTools>) => void;
}

export const EMPTY_TOOLS: Required<ChatTools> = { disabledServers: [], disabledTools: [] };

export const useMcpUi = create<McpUiState>((set) => ({
  asks: [],
  progress: {},
  draftTools: EMPTY_TOOLS,
  addAsk: (i) => set((s) => (s.asks.some((a) => a.id === i.id) ? s : { asks: [...s.asks, i] })),
  closeAsk: (id) => set((s) => ({ asks: s.asks.filter((a) => a.id !== id) })),
  setAsks: (all) =>
    set((s) => {
      // Keep anything an event added while the list was loading.
      const ids = new Set(all.map((a) => a.id));
      return { asks: [...all, ...s.asks.filter((a) => !ids.has(a.id))] };
    }),
  setProgress: (token, p) => set((s) => ({ progress: { ...s.progress, [token]: p } })),
  clearProgress: (token) =>
    set((s) => {
      const next = { ...s.progress };
      delete next[token];
      return { progress: next };
    }),
  setDraftTools: (t) => set({ draftTools: t }),
}));

export function resetMcpUi(): void {
  useMcpUi.setState({ asks: [], progress: {}, draftTools: EMPTY_TOOLS });
}
