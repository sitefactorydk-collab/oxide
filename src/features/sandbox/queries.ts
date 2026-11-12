import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { commands, events, type SandboxChange } from "@/bindings";
import { DEFAULT_SETTINGS, useSettings } from "@/features/settings/queries";
import { unwrap } from "@/lib/api";

export const sandboxKeys = {
  all: ["sandbox"] as const,
  docker: ["sandbox", "docker"] as const,
  chat: (conversationId: string) => ["sandbox", "chat", conversationId] as const,
  files: (conversationId: string, path?: string) =>
    path === undefined
      ? (["sandbox", "files", conversationId] as const)
      : (["sandbox", "files", conversationId, path] as const),
};

/** Docker and the sandbox's images. Read again whenever a page using it opens. */
export function useDockerStatus(enabled = true) {
  return useQuery({
    queryKey: sandboxKeys.docker,
    queryFn: () => unwrap(commands.sandboxDockerStatus(false)),
    staleTime: 0,
    enabled,
  });
}

/** Checks Docker again now (the "Check again" button). */
export function useRefreshDocker() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => unwrap(commands.sandboxDockerStatus(true)),
    onSuccess: (st) => qc.setQueryData(sandboxKeys.docker, st),
  });
}

/** Downloads the images. Resolves when they're there; the status shows the download. */
export function usePrepareSandbox() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => unwrap(commands.sandboxPrepare()),
    onSettled: () => qc.invalidateQueries({ queryKey: sandboxKeys.docker }),
  });
}

/** A chat's sandbox (state, workspace, running commands). */
export function useChatSandbox(conversationId: string, enabled = true) {
  return useQuery({
    queryKey: sandboxKeys.chat(conversationId),
    queryFn: () => unwrap(commands.chatSandbox(conversationId)),
    staleTime: 0,
    enabled,
  });
}

/** One folder of a chat's workspace; `null` while the sandbox isn't running. */
export function useSandboxFiles(conversationId: string, path: string, enabled = true) {
  return useQuery({
    queryKey: sandboxKeys.files(conversationId, path),
    queryFn: () => unwrap(commands.sandboxFiles(conversationId, path, false)),
    staleTime: 0,
    enabled,
  });
}

function useChatAction<V>(conversationId: string, run: (v: V) => Promise<unknown>) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: run,
    onSettled: () =>
      Promise.all([
        qc.invalidateQueries({ queryKey: sandboxKeys.chat(conversationId) }),
        qc.invalidateQueries({ queryKey: sandboxKeys.files(conversationId) }),
      ]),
  });
}

/** Starts a stopped sandbox to look at its files. */
export function useStartSandbox(conversationId: string) {
  return useChatAction(conversationId, (path: string) =>
    unwrap(commands.sandboxFiles(conversationId, path, true)),
  );
}

export function useStopSandbox(conversationId: string) {
  return useChatAction(conversationId, () => unwrap(commands.sandboxStop(conversationId)));
}

export function useResetSandbox(conversationId: string) {
  return useChatAction(conversationId, () => unwrap(commands.sandboxReset(conversationId)));
}

/** Picks a file (or folder) and copies it into `dest`. Resolves to its sandbox path or null. */
export function useCopyIntoSandbox(conversationId: string) {
  return useChatAction(conversationId, (v: { dest: string; folder: boolean }) =>
    unwrap(commands.sandboxCopyIn(conversationId, v.dest, v.folder)),
  );
}

/** Picks a folder and saves `path` from the sandbox there. Resolves to where it went, or null. */
export function useSaveFromSandbox(conversationId: string) {
  return useMutation({
    mutationFn: (path: string) => unwrap(commands.sandboxSave(conversationId, path)),
  });
}

/** What to read again after a `sandboxChanged` event. */
export function invalidateFor(change: SandboxChange): readonly (readonly string[])[] {
  return change.conversationId === null
    ? [sandboxKeys.docker]
    : [sandboxKeys.chat(change.conversationId), sandboxKeys.files(change.conversationId)];
}

/** App-wide listener for `sandboxChanged`. Mounted once in `AppLayout`. */
export function useSandboxEvents(): void {
  const qc = useQueryClient();
  useEffect(() => {
    let disposed = false;
    let stop: (() => void) | undefined;
    events.sandboxChanged
      .listen(({ payload }) => {
        for (const queryKey of invalidateFor(payload)) {
          void qc.invalidateQueries({ queryKey });
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

/** Whether the panel shows a Sandbox section for this chat. */
export function useShowSandbox(id: string): boolean {
  const { data } = useSettings();
  const enabled = (data ?? DEFAULT_SETTINGS).sandbox.enabled;
  const { data: sb } = useChatSandbox(id, enabled);
  return enabled || (sb !== undefined && sb.state !== "none");
}
