import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  commands,
  type AppCapability,
  type AppPrefs,
  type GenerationKind,
  type UtilityApp,
} from "@/bindings";
import { unwrap } from "@/lib/api";

export const appKeys = {
  all: ["apps"] as const,
  list: () => [...appKeys.all, "list"] as const,
  bundle: (id: string) => [...appKeys.all, "bundle", id] as const,
  prefs: (id: string) => [...appKeys.all, "prefs", id] as const,
};

export function useApps() {
  return useQuery({ queryKey: appKeys.list(), queryFn: () => unwrap(commands.listApps()) });
}

export function useApp(id: string | undefined): UtilityApp | undefined {
  const { data } = useApps();
  return id ? data?.find((a) => a.manifest.id === id) : undefined;
}

export function useAppBundle(id: string | undefined) {
  return useQuery({
    queryKey: appKeys.bundle(id ?? ""),
    queryFn: () => unwrap(commands.appBundle(id ?? "")),
    enabled: Boolean(id),
    // An own app is read from disk each time: edits show on reload.
    staleTime: 0,
  });
}

export function useAppPrefs(id: string) {
  return useQuery({ queryKey: appKeys.prefs(id), queryFn: () => unwrap(commands.appPrefs(id)) });
}

export function useCloneApp() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => unwrap(commands.cloneApp(id)),
    onSuccess: () => qc.invalidateQueries({ queryKey: appKeys.list() }),
  });
}

export function useDeleteApp() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => unwrap(commands.deleteApp(id)),
    onSuccess: () => qc.invalidateQueries({ queryKey: appKeys.all }),
  });
}

export function useExportApp() {
  return useMutation({ mutationFn: (id: string) => unwrap(commands.exportApp(id)) });
}

export function usePickImport() {
  return useMutation({ mutationFn: () => unwrap(commands.pickAppImport()) });
}

export function useInstallImport() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (token: string) => unwrap(commands.installAppImport(token)),
    onSuccess: () => qc.invalidateQueries({ queryKey: appKeys.all }),
  });
}

export function useSaveAppFile() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (v: { id: string; path: string; content: string }) =>
      unwrap(commands.saveAppFile(v.id, v.path, v.content)),
    onSuccess: (_, v) =>
      Promise.all([
        qc.invalidateQueries({ queryKey: appKeys.list() }),
        qc.invalidateQueries({ queryKey: appKeys.bundle(v.id) }),
      ]),
  });
}

export function useSaveAppPrefs(id: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (prefs: AppPrefs) => unwrap(commands.saveAppPrefs(id, prefs)),
    onSuccess: () => qc.invalidateQueries({ queryKey: appKeys.prefs(id) }),
  });
}

export function useGrantCapability() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (v: { id: string; capability: AppCapability }) =>
      unwrap(commands.grantAppCapability(v.id, v.capability)),
    onSuccess: (_, v) => qc.invalidateQueries({ queryKey: appKeys.prefs(v.id) }),
  });
}

export function useSetAppKey(id: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (v: { kind: GenerationKind; key: string | null }) =>
      unwrap(commands.setAppKey(id, v.kind, v.key)),
    onSuccess: () => qc.invalidateQueries({ queryKey: appKeys.prefs(id) }),
  });
}

export function useClearAppStorage(id: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => unwrap(commands.clearAppStorage(id)),
    onSuccess: () => qc.invalidateQueries({ queryKey: appKeys.prefs(id) }),
  });
}

/** What each capability lets an app do, for approval cards and the settings dialog. */
export const CAPABILITY_TEXT: Record<AppCapability, { label: string; ask: string }> = {
  text: { label: "AI text", ask: "use your AI models to write text (this may cost money)" },
  image: { label: "Image generation", ask: "generate images (this may cost money)" },
  video: { label: "Video generation", ask: "generate videos (this may cost money)" },
  music: { label: "Music generation", ask: "generate music (this may cost money)" },
  webSearch: { label: "Web search", ask: "search the web" },
  webFetch: { label: "Reading web pages", ask: "read web pages" },
  connectors: { label: "Connectors", ask: "use the tools of your connectors" },
  python: { label: "Python", ask: "run Python code on this computer (not sandboxed)" },
  sandbox: { label: "Docker sandbox", ask: "run commands in its Docker sandbox" },
  browser: { label: "Browser agent", ask: "control the browser agent" },
  chrome: { label: "Chrome", ask: "control your paired Chrome" },
};
