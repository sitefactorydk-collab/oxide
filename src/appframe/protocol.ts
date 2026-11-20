/**
 * Messages between a Utility App's frame and the main window (Phase 20).
 *
 * The frame runs the app in an `<iframe>` sandboxed without `allow-same-origin` and with no
 * network, so the only way out is `postMessage` to the main window, which checks each
 * message's source window and answers SDK calls through Oxide's backend.
 */

/** A Utility App file the frame loads. */
export interface FrameFile {
  path: string;
  content: string;
}

export type Theme = "light" | "dark";

export type ConsoleLevel = "log" | "info" | "warn" | "error" | "debug";

/** Error codes an SDK call can fail with (the backend's `AppErrorCode`, plus `denied`). */
export type SdkErrorCode =
  "needsPermission" | "undeclared" | "denied" | "cancelled" | "unknown" | "failed";

export interface SdkError {
  code: SdkErrorCode;
  message: string;
}

/** Frame → main window. */
export type FrameToHost =
  | { source: "oxide-app"; type: "ready" }
  | { source: "oxide-app"; type: "rendered" }
  | { source: "oxide-app"; type: "error"; message: string }
  /** A console line of the app (the builder's Preview shows them to its model). */
  | { source: "oxide-app"; type: "console"; level: ConsoleLevel; text: string }
  /** The answer to a `snapshot` request: a PNG `data:` URL, or why there is none. */
  | { source: "oxide-app"; type: "snapshot"; id: string; dataUrl?: string; error?: string }
  | { source: "oxide-app"; type: "call"; id: string; method: string; params: unknown }
  | { source: "oxide-app"; type: "cancel"; id: string };

/** Main window → frame. */
export type HostToFrame =
  | {
      source: "oxide-host";
      type: "load";
      app: { id: string; name: string };
      entry: string;
      files: FrameFile[];
      theme: Theme;
    }
  | { source: "oxide-host"; type: "theme"; theme: Theme }
  /** Take a picture of the app (the builder's `app_screenshot`). */
  | { source: "oxide-host"; type: "snapshot"; id: string }
  | { source: "oxide-host"; type: "result"; id: string; ok: true; value: unknown }
  | { source: "oxide-host"; type: "result"; id: string; ok: false; error: SdkError }
  | { source: "oxide-host"; type: "event"; id: string; event: unknown };

export function isFrameToHost(data: unknown): data is FrameToHost {
  return (
    typeof data === "object" &&
    data !== null &&
    (data as { source?: unknown }).source === "oxide-app" &&
    typeof (data as { type?: unknown }).type === "string"
  );
}

export function isHostToFrame(data: unknown): data is HostToFrame {
  return (
    typeof data === "object" &&
    data !== null &&
    (data as { source?: unknown }).source === "oxide-host" &&
    typeof (data as { type?: unknown }).type === "string"
  );
}
