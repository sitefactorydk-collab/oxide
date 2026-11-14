import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { commands, events, type UpdateStatus } from "@/bindings";
import { unwrap } from "@/lib/api";
import { toast } from "@/lib/toast";

export const dataKeys = {
  locations: ["data", "locations"] as const,
  update: ["data", "update"] as const,
};

/** Oxide's folders and the data folder's size. */
export function useDataLocations() {
  return useQuery({
    queryKey: dataKeys.locations,
    queryFn: () => unwrap(commands.dataLocations()),
    staleTime: 30_000,
  });
}

export async function openDataFolder(): Promise<void> {
  await unwrap(commands.openDataFolder());
}

export async function revealDataExport(): Promise<void> {
  await unwrap(commands.revealDataExport());
}

/** Asks where to save and writes the zip; resolves to `null` when the dialog was cancelled. */
export function useExportAll() {
  return useMutation({ mutationFn: () => unwrap(commands.exportAllData()) });
}

/**
 * Deletes everything. On success the window's own storage (theme, last model, drafts) is
 * cleared too; Oxide restarts a moment later.
 */
export function useDeleteAll() {
  return useMutation({
    mutationFn: (confirm: string) => unwrap(commands.deleteAllData(confirm)),
    onSuccess: () => {
      try {
        localStorage.clear();
        sessionStorage.clear();
      } catch {
        // Storage may be unavailable; the backend's data is gone either way.
      }
    },
  });
}

/** The updater's state, kept fresh by `updateChanged`. */
export function useUpdateStatus() {
  return useQuery({
    queryKey: dataKeys.update,
    queryFn: () => commands.updateStatus(),
    staleTime: Infinity,
  });
}

export function useCheckForUpdate() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => unwrap(commands.checkForUpdate()),
    onSuccess: (s) => qc.setQueryData(dataKeys.update, s),
  });
}

export function useInstallUpdate() {
  return useMutation({ mutationFn: () => unwrap(commands.installUpdate()) });
}

/**
 * App-wide: keeps the update status current and says so once when a new version turns up
 * (the automatic check runs in the background).
 */
export function useUpdateEvents(): void {
  const qc = useQueryClient();
  useEffect(() => {
    let disposed = false;
    let stop: (() => void) | null = null;
    events.updateChanged
      .listen(({ payload }) => {
        const before = qc.getQueryData<UpdateStatus>(dataKeys.update);
        qc.setQueryData(dataKeys.update, payload);
        const version = payload.available?.version;
        if (payload.phase === "available" && version && before?.available?.version !== version) {
          toast(`Oxide ${version} is available. Install it from Settings → About.`);
        }
      })
      .then((unlisten) => (disposed ? unlisten() : (stop = unlisten)))
      .catch(() => undefined);
    return () => {
      disposed = true;
      stop?.();
    };
  }, [qc]);
}
