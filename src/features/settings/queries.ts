import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import {
  commands,
  type AppSettings,
  type BrowserSettings,
  type ChatFont,
  type ChromeSettings,
  type ContextSettings,
  type DesktopSettings,
  type MemorySettings,
  type ModelSettings,
  type ProjectSettings,
  type PythonSettings,
  type SandboxSettings,
  type ShellSettings,
  type ThemePref,
  type ToolSettings,
  type WebSearchSettings,
} from "@/bindings";
import { contextKeys } from "@/features/context/queries";
import { desktopKeys } from "@/features/desktop/keys";
import { projectKeys } from "@/features/projects/queries";
import { unwrap } from "@/lib/api";
import { useUi } from "@/stores/ui";

export const settingsKeys = { app: ["settings", "app"] as const };

export const DEFAULT_SETTINGS: AppSettings = {
  profile: { name: "" },
  appearance: { theme: "system", chatFont: "serif" },
  models: { showUnsupported: false, autoTitle: true, titleModel: null, thinkingBudget: 8000 },
  tools: { autoAllowReadOnly: false, askBeforeWeb: false, maxSteps: 25 },
  webSearch: { provider: "none", searxngUrl: "", anthropicNative: true, maxResults: 5 },
  projects: { knowledgeBudget: 40000 },
  memory: { enabled: false, model: null },
  desktop: {
    closeToTray: true,
    quickEntry: true,
    quickEntryShortcut: "Alt+Space",
    launchAtLogin: false,
    notifications: true,
  },
  shell: { enabled: false, allowlist: [], timeoutSecs: 120 },
  python: { enabled: true, timeoutSecs: 120 },
  sandbox: {
    enabled: false,
    image: "python:3.12-bookworm",
    filesImage: "mcp/filesystem",
    cpus: 2,
    memoryMb: 2048,
    network: false,
    storage: "volume",
    folder: "",
    idleMinutes: 15,
    timeoutSecs: 600,
    ask: true,
  },
  browser: {
    enabled: false,
    source: "auto",
    cdpUrl: "http://127.0.0.1:9222",
    showWindow: false,
    ask: true,
    maxSteps: 25,
    taskTimeoutSecs: 600,
    idleMinutes: 15,
    agentModel: null,
  },
  chrome: { enabled: false, port: 47820, ask: true },
  context: {
    compaction: true,
    threshold: 75,
    compactionModel: null,
    anthropicClearing: true,
    windows: [],
  },
};

export function useSettings() {
  return useQuery({
    queryKey: settingsKeys.app,
    queryFn: () => unwrap(commands.getSettings()),
  });
}

/**
 * Updates settings: applies the change at once (query cache + the UI cache that drives the
 * theme), then saves. On failure the stored settings are reloaded.
 */
export function useUpdateSettings() {
  const qc = useQueryClient();
  const mutation = useMutation({
    mutationFn: (next: AppSettings) => unwrap(commands.saveSettings(next)),
    // Don't write the response back: a slower earlier save could overwrite a newer edit.
    onError: () => qc.invalidateQueries({ queryKey: settingsKeys.app }),
    // Whether a project's knowledge is used in full or searched depends on the budget.
    // (On the mutation, not on `mutate`, so it runs even after Settings has closed.)
    // The shortcut and start at login are applied when saved; Settings → Desktop shows the outcome.
    onSuccess: () =>
      Promise.all([
        qc.invalidateQueries({ queryKey: projectKeys.allKnowledge() }),
        qc.invalidateQueries({ queryKey: desktopKeys.status }),
        // The meter's window and threshold come from Settings → Models.
        qc.invalidateQueries({ queryKey: contextKeys.all }),
      ]),
  });
  return (change: (s: AppSettings) => AppSettings) => {
    const current = qc.getQueryData<AppSettings>(settingsKeys.app) ?? DEFAULT_SETTINGS;
    const next = change(current);
    qc.setQueryData(settingsKeys.app, next);
    cacheForFirstPaint(next);
    mutation.mutate(next);
  };
}

/** Mirrors the appearance and name into `useUi`, which persists them for the first frame. */
function cacheForFirstPaint(s: AppSettings) {
  useUi.setState({
    userName: s.profile.name,
    theme: s.appearance.theme,
    chatFont: s.appearance.chatFont,
  });
}

function isDefault(s: AppSettings): boolean {
  return JSON.stringify(s) === JSON.stringify(DEFAULT_SETTINGS);
}

/**
 * Keeps the first-paint cache in step with stored settings. Once, it also carries over
 * preferences saved by Phase 1 builds (which only lived in localStorage).
 */
export function useSettingsSync(): void {
  const { data } = useSettings();
  const update = useUpdateSettings();
  useEffect(() => {
    if (!data) return;
    const ui = useUi.getState();
    if (!ui.settingsMigrated) {
      useUi.setState({ settingsMigrated: true });
      const legacy: AppSettings = {
        ...data,
        profile: { name: ui.userName },
        appearance: { theme: ui.theme, chatFont: ui.chatFont },
      };
      if (isDefault(data) && !isDefault(legacy)) {
        update(() => legacy);
        return;
      }
    }
    cacheForFirstPaint(data);
    // `update` is a fresh closure each render; only stored data should re-run this.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [data]);
}

export function useSetTheme() {
  const update = useUpdateSettings();
  return (theme: ThemePref) => update((s) => ({ ...s, appearance: { ...s.appearance, theme } }));
}

export function useSetChatFont() {
  const update = useUpdateSettings();
  return (chatFont: ChatFont) =>
    update((s) => ({ ...s, appearance: { ...s.appearance, chatFont } }));
}

export function useSetProfileName() {
  const update = useUpdateSettings();
  return (name: string) => update((s) => ({ ...s, profile: { ...s.profile, name } }));
}

export function useSetModelSettings() {
  const update = useUpdateSettings();
  return (patch: Partial<ModelSettings>) =>
    update((s) => ({ ...s, models: { ...s.models, ...patch } }));
}

export function useSetContextSettings() {
  const update = useUpdateSettings();
  return (patch: Partial<ContextSettings>) =>
    update((s) => ({ ...s, context: { ...s.context, ...patch } }));
}

export function useSetShellSettings() {
  const update = useUpdateSettings();
  return (patch: Partial<ShellSettings>) =>
    update((s) => ({ ...s, shell: { ...s.shell, ...patch } }));
}

export function useSetPythonSettings() {
  const update = useUpdateSettings();
  return (patch: Partial<PythonSettings>) =>
    update((s) => ({ ...s, python: { ...s.python, ...patch } }));
}

export function useSetSandboxSettings() {
  const update = useUpdateSettings();
  return (patch: Partial<SandboxSettings>) =>
    update((s) => ({ ...s, sandbox: { ...s.sandbox, ...patch } }));
}

export function useSetBrowserSettings() {
  const update = useUpdateSettings();
  return (patch: Partial<BrowserSettings>) =>
    update((s) => ({ ...s, browser: { ...s.browser, ...patch } }));
}

export function useSetChromeSettings() {
  const update = useUpdateSettings();
  return (patch: Partial<ChromeSettings>) =>
    update((s) => ({ ...s, chrome: { ...s.chrome, ...patch } }));
}

export function useSetToolSettings() {
  const update = useUpdateSettings();
  return (patch: Partial<ToolSettings>) =>
    update((s) => ({ ...s, tools: { ...s.tools, ...patch } }));
}

export function useSetProjectSettings() {
  const update = useUpdateSettings();
  return (patch: Partial<ProjectSettings>) =>
    update((s) => ({ ...s, projects: { ...s.projects, ...patch } }));
}

export function useSetMemorySettings() {
  const update = useUpdateSettings();
  return (patch: Partial<MemorySettings>) =>
    update((s) => ({ ...s, memory: { ...s.memory, ...patch } }));
}

export function useSetWebSearchSettings() {
  const update = useUpdateSettings();
  return (patch: Partial<WebSearchSettings>) =>
    update((s) => ({ ...s, webSearch: { ...s.webSearch, ...patch } }));
}

export function useSetDesktopSettings() {
  const update = useUpdateSettings();
  return (patch: Partial<DesktopSettings>) =>
    update((s) => ({ ...s, desktop: { ...s.desktop, ...patch } }));
}
