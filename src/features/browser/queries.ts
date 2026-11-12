import { useMutation, useQuery, useQueryClient, type QueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { create } from "zustand";
import { commands, events, type BrowserFrame, type BrowserStatus } from "@/bindings";
import { DEFAULT_SETTINGS, useSettings } from "@/features/settings/queries";
import { unwrap } from "@/lib/api";

export const browserKeys = { status: ["browser", "status"] as const };

/** The browser agent: what is installed, which browser chats get, which chats have one open. */
export function useBrowserStatus(enabled = true) {
  return useQuery({
    queryKey: browserKeys.status,
    queryFn: () => unwrap(commands.browserStatus()),
    // Read again whenever a page using it opens: chats open and close browsers on their own.
    staleTime: 0,
    enabled,
  });
}

export function setBrowserStatus(qc: QueryClient, status: BrowserStatus): void {
  qc.setQueryData(browserKeys.status, status);
}

/** Setup returns at once; progress arrives as `browserChanged` events. */
export function useBrowserSetup() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (chromium: boolean) => unwrap(commands.browserSetup(chromium)),
    onSettled: () => qc.invalidateQueries({ queryKey: browserKeys.status }),
  });
}

export function useCloseBrowser() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (conversationId: string) => unwrap(commands.browserClose(conversationId)),
    onSettled: () => qc.invalidateQueries({ queryKey: browserKeys.status }),
  });
}

export function useRemoveChromium() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => unwrap(commands.browserRemoveChromium()),
    onSettled: () => qc.invalidateQueries({ queryKey: browserKeys.status }),
  });
}

/** Live screenshots of running browser calls, by tool call id (kept for the session). */
interface BrowserLiveState {
  frames: Record<string, BrowserFrame>;
  setFrame: (f: BrowserFrame) => void;
}

/** Most calls whose last screenshot is kept (older ones are dropped). */
const KEEP_FRAMES = 40;

export const useBrowserLive = create<BrowserLiveState>((set) => ({
  frames: {},
  setFrame: (f) =>
    set((s) => {
      // A frame without a picture keeps the call's last picture.
      const prev = s.frames[f.toolUseId];
      const next = { ...f, image: f.image ?? prev?.image ?? null, url: f.url ?? prev?.url ?? null };
      const entries = Object.entries({ ...s.frames, [f.toolUseId]: next });
      return { frames: Object.fromEntries(entries.slice(-KEEP_FRAMES)) };
    }),
}));

/** The last live screenshot of a call, if one arrived. */
export function useBrowserFrame(toolUseId: string): BrowserFrame | undefined {
  return useBrowserLive((s) => s.frames[toolUseId]);
}

/**
 * App-wide listeners for `browserChanged` (into the query cache) and `browserLive` (into the
 * live store). Mounted once in `AppLayout`.
 */
export function useBrowserEvents(): void {
  const qc = useQueryClient();
  useEffect(() => {
    let disposed = false;
    const stops: Array<() => void> = [];
    const keep = (unlisten: () => void) => (disposed ? unlisten() : stops.push(unlisten));
    events.browserChanged
      .listen(({ payload }) => setBrowserStatus(qc, payload))
      .then(keep)
      .catch(() => undefined);
    events.browserLive
      .listen(({ payload }) => useBrowserLive.getState().setFrame(payload))
      .then(keep)
      .catch(() => undefined);
    return () => {
      disposed = true;
      for (const s of stops) s();
    };
  }, [qc]);
}

/** Whether the browser is on in Settings. */
export function useBrowserEnabled(): boolean {
  return (useSettings().data ?? DEFAULT_SETTINGS).browser.enabled;
}

/** Whether a chat's browser is open now. */
export function useChatBrowserOpen(conversationId: string): boolean {
  const enabled = useBrowserEnabled();
  const { data } = useBrowserStatus(enabled);
  return data?.openChats.includes(conversationId) ?? false;
}
