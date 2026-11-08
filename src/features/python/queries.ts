import { useMutation, useQuery, useQueryClient, type QueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { commands, events, type PythonStatus } from "@/bindings";
import { unwrap } from "@/lib/api";

export const pythonKeys = { status: ["python", "status"] as const };

/** The managed Python environment: its state while it installs, and what is in it once ready. */
export function usePythonStatus(enabled = true) {
  return useQuery({
    queryKey: pythonKeys.status,
    queryFn: () => unwrap(commands.pythonStatus()),
    enabled,
  });
}

/**
 * True while the environment is being installed or repaired (shown as "Setting up Python").
 * `enabled: false` skips reading the status (for tool calls that aren't Python).
 */
export function usePythonInstalling(enabled = true): boolean {
  return usePythonStatus(enabled).data?.state === "installing";
}

export function setPythonStatus(qc: QueryClient, status: PythonStatus): void {
  qc.setQueryData(pythonKeys.status, status);
}

/**
 * App-wide listener for `pythonChanged`: every status the backend sends goes into the query
 * cache. Mounted once in `AppLayout`.
 */
export function usePythonEvents(): void {
  const qc = useQueryClient();
  useEffect(() => {
    let disposed = false;
    let stop: (() => void) | undefined;
    events.pythonChanged
      .listen(({ payload }) => setPythonStatus(qc, payload))
      .then((unlisten) => (disposed ? unlisten() : (stop = unlisten)))
      .catch(() => undefined);
    return () => {
      disposed = true;
      stop?.();
    };
  }, [qc]);
}

/**
 * Setup, repair and reset return at once; progress arrives as events. Afterwards the status is
 * read again so the page is right even when an event was missed.
 */
function usePythonAction(run: () => Promise<unknown>) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: run,
    onSettled: () => qc.invalidateQueries({ queryKey: pythonKeys.status }),
  });
}

export function usePythonSetup() {
  return usePythonAction(() => unwrap(commands.pythonSetup()));
}

export function usePythonRepair() {
  return usePythonAction(() => unwrap(commands.pythonRepair()));
}

export function usePythonReset() {
  return usePythonAction(() => unwrap(commands.pythonReset()));
}

export function openPythonFolder(): Promise<null> {
  return unwrap(commands.openPythonFolder());
}
