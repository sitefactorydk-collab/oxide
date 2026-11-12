import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { commands, events } from "@/bindings";
import { DEFAULT_SETTINGS, useSettings } from "@/features/settings/queries";
import { unwrap } from "@/lib/api";

export const chromeKeys = { status: ["chrome", "status"] as const };

/** The Chrome link: listening, the connected Chrome, paired browsers, a waiting code. */
export function useChromeStatus() {
  return useQuery({
    queryKey: chromeKeys.status,
    queryFn: () => unwrap(commands.chromeStatus()),
    staleTime: 0,
  });
}

export function useStartPairing() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => unwrap(commands.chromeStartPairing()),
    onSettled: () => qc.invalidateQueries({ queryKey: chromeKeys.status }),
  });
}

export function useCancelPairing() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => unwrap(commands.chromeCancelPairing()),
    onSettled: () => qc.invalidateQueries({ queryKey: chromeKeys.status }),
  });
}

export function useForgetBrowser() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (browserId: string) => unwrap(commands.chromeForget(browserId)),
    onSettled: () => qc.invalidateQueries({ queryKey: chromeKeys.status }),
  });
}

export function useOpenExtensionFolder() {
  return useMutation({ mutationFn: () => unwrap(commands.chromeOpenExtensionFolder()) });
}

/** App-wide listener for `chromeChanged` (into the query cache). Mounted once in `AppLayout`. */
export function useChromeEvents(): void {
  const qc = useQueryClient();
  useEffect(() => {
    let disposed = false;
    let stop: (() => void) | null = null;
    events.chromeChanged
      .listen(({ payload }) => qc.setQueryData(chromeKeys.status, payload))
      .then((unlisten) => (disposed ? unlisten() : (stop = unlisten)))
      .catch(() => undefined);
    return () => {
      disposed = true;
      stop?.();
    };
  }, [qc]);
}

/** Whether Chrome is on in Settings. */
export function useChromeEnabled(): boolean {
  return (useSettings().data ?? DEFAULT_SETTINGS).chrome.enabled;
}
