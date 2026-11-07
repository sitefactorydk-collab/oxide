import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { inTauri } from "@/lib/platform";

/**
 * Thin wrappers over Tauri's window/webview APIs. They no-op outside Tauri (browser preview,
 * tests). These are core window APIs, not app commands, so they don't go through `@/bindings`.
 */
type Unlisten = () => void;

export async function minimizeWindow(): Promise<void> {
  if (inTauri) await getCurrentWindow().minimize();
}

export async function toggleMaximizeWindow(): Promise<void> {
  if (inTauri) await getCurrentWindow().toggleMaximize();
}

export async function closeWindow(): Promise<void> {
  if (inTauri) await getCurrentWindow().close();
}

export async function isWindowMaximized(): Promise<boolean> {
  return inTauri ? getCurrentWindow().isMaximized() : false;
}

export async function onWindowResized(cb: () => void): Promise<Unlisten> {
  if (!inTauri) return () => {};
  return getCurrentWindow().onResized(() => cb());
}

/** Calls `cb` when this window gains (`true`) or loses (`false`) focus. */
export async function onWindowFocusChanged(cb: (focused: boolean) => void): Promise<Unlisten> {
  if (!inTauri) return () => {};
  return getCurrentWindow().onFocusChanged(({ payload }) => cb(payload));
}

/** Keeps native window chrome (macOS/Linux decorations) in step with the app theme. */
export async function setWindowTheme(theme: "light" | "dark" | null): Promise<void> {
  if (inTauri) await getCurrentWindow().setTheme(theme);
}

export type FileDropEvent =
  | { type: "enter"; paths: string[] }
  | { type: "over" }
  | { type: "leave" }
  | { type: "drop"; paths: string[] };

/** Native file drag-and-drop (Tauri intercepts HTML5 drops by default and gives real paths). */
export async function onNativeFileDrop(cb: (e: FileDropEvent) => void): Promise<Unlisten> {
  if (!inTauri) return () => {};
  return getCurrentWebview().onDragDropEvent((event) => {
    const p = event.payload;
    switch (p.type) {
      case "enter":
        cb({ type: "enter", paths: p.paths });
        break;
      case "over":
        cb({ type: "over" });
        break;
      case "drop":
        cb({ type: "drop", paths: p.paths });
        break;
      default:
        cb({ type: "leave" });
    }
  });
}
