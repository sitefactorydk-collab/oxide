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

/** Where a drag is, in the window's CSS pixels. */
export interface DropPoint {
  x: number;
  y: number;
}

export type FileDropEvent =
  | { type: "enter"; paths: string[]; position: DropPoint }
  | { type: "over"; position: DropPoint }
  | { type: "leave" }
  | { type: "drop"; paths: string[]; position: DropPoint };

/** Whether a drag at `p` is over an element that takes native drops itself (an app frame). */
export function overOwnDropZone(p: DropPoint | undefined): boolean {
  if (!p || typeof document.elementFromPoint !== "function") return false;
  return Boolean(document.elementFromPoint(p.x, p.y)?.closest("[data-native-drop-zone]"));
}

/** Native file drag-and-drop (Tauri intercepts HTML5 drops by default and gives real paths). */
export async function onNativeFileDrop(cb: (e: FileDropEvent) => void): Promise<Unlisten> {
  if (!inTauri) return () => {};
  // Tauri reports physical pixels of the webview; the page works in CSS pixels.
  const at = (p: { x: number; y: number }): DropPoint => {
    const scale = window.devicePixelRatio || 1;
    return { x: p.x / scale, y: p.y / scale };
  };
  return getCurrentWebview().onDragDropEvent((event) => {
    const p = event.payload;
    switch (p.type) {
      case "enter":
        cb({ type: "enter", paths: p.paths, position: at(p.position) });
        break;
      case "over":
        cb({ type: "over", position: at(p.position) });
        break;
      case "drop":
        cb({ type: "drop", paths: p.paths, position: at(p.position) });
        break;
      default:
        cb({ type: "leave" });
    }
  });
}
