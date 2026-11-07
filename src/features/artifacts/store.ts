import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";
import { safeLocalStorage } from "@/lib/storage";

export const PANEL_MIN = 360;
export const PANEL_DEFAULT = 560;

/** Narrowest the chat column gets while the panel is open. */
export const CHAT_MIN = 380;

export function clampPanelWidth(w: number, available = Number.POSITIVE_INFINITY): number {
  if (!Number.isFinite(w)) return PANEL_DEFAULT;
  const max = Math.max(PANEL_MIN, available - CHAT_MIN);
  return Math.round(Math.min(max, Math.max(PANEL_MIN, w)));
}

/** What the artifact panel shows. */
export interface PanelTarget {
  conversationId: string;
  /** The artifact, once it's saved. */
  artifactId: string | null;
  /** The version picked (`null`: the newest). */
  version: number | null;
  /** An `artifact` tool call being written, shown live until its result arrives. */
  liveCallId: string | null;
}

export type PanelView = "preview" | "code";

interface ArtifactPanelState {
  target: PanelTarget | null;
  width: number;
  fullscreen: boolean;
  view: PanelView;
  /** A live call the user closed the panel on: don't open it again for that call. */
  dismissedCall: string | null;
  open: (conversationId: string, artifactId: string, version?: number | null) => void;
  /** Shows a call while it's written (unless the user closed the panel on it; `force`: anyway). */
  openLive: (conversationId: string, callId: string, force?: boolean) => void;
  /** A call finished: if the panel shows it, switch to the saved artifact. */
  settle: (conversationId: string, callId: string, artifactId: string, version: number) => void;
  close: () => void;
  setVersion: (version: number | null) => void;
  setView: (view: PanelView) => void;
  setWidth: (w: number) => void;
  setFullscreen: (on: boolean) => void;
}

export const useArtifactPanel = create<ArtifactPanelState>()(
  persist(
    (set, get) => ({
      target: null,
      width: PANEL_DEFAULT,
      fullscreen: false,
      view: "preview",
      dismissedCall: null,
      open: (conversationId, artifactId, version = null) =>
        set({
          target: { conversationId, artifactId, version, liveCallId: null },
          view: "preview",
        }),
      openLive: (conversationId, callId, force = false) => {
        const { target, dismissedCall } = get();
        if ((!force && dismissedCall === callId) || target?.liveCallId === callId) return;
        set({
          target: { conversationId, artifactId: null, version: null, liveCallId: callId },
          view: "code",
        });
      },
      settle: (conversationId, callId, artifactId, version) => {
        const { target, dismissedCall } = get();
        if (
          target?.liveCallId === callId ||
          (target?.conversationId === conversationId && target.artifactId === artifactId)
        ) {
          set({
            target: { conversationId, artifactId, version, liveCallId: null },
            view: "preview",
          });
        } else if (!target && dismissedCall !== callId) {
          set({
            target: { conversationId, artifactId, version, liveCallId: null },
            view: "preview",
          });
        }
      },
      close: () =>
        set((s) => ({
          target: null,
          fullscreen: false,
          dismissedCall: s.target?.liveCallId ?? s.dismissedCall,
        })),
      setVersion: (version) => set((s) => (s.target ? { target: { ...s.target, version } } : s)),
      setView: (view) => set({ view }),
      setWidth: (w) => set({ width: clampPanelWidth(w) }),
      setFullscreen: (fullscreen) => set({ fullscreen }),
    }),
    {
      name: "oxide-artifact-panel",
      storage: createJSONStorage(safeLocalStorage),
      partialize: (s) => ({ width: s.width }),
    },
  ),
);

/** Resets the panel (tests). */
export function resetArtifactPanel(): void {
  useArtifactPanel.setState({
    target: null,
    width: PANEL_DEFAULT,
    fullscreen: false,
    view: "preview",
    dismissedCall: null,
  });
}
