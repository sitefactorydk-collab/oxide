import { inTauri, platform } from "@/lib/platform";

/**
 * Where the sandboxed artifact frame loads from: Oxide's `oxide-artifact` protocol (a
 * different origin from the app; on Windows WebView2 serves custom schemes as
 * `http://<scheme>.localhost`). In a plain browser (tests, `npm run dev`) it's the Vite page.
 */
export const FRAME_URL = !inTauri
  ? "/artifact-frame.html"
  : platform === "windows"
    ? "http://oxide-artifact.localhost/artifact-frame.html"
    : "oxide-artifact://localhost/artifact-frame.html";

/**
 * The frame's sandbox. No `allow-same-origin`: the frame's origin is opaque, so it can't
 * reach the app, its storage or its IPC. Forms may fire submit events (React apps use them),
 * but the frame's CSP (`form-action 'none'`) stops them from going anywhere.
 */
export const FRAME_SANDBOX = "allow-scripts allow-forms allow-modals allow-downloads";

/** Messages the frame sends. */
export type FrameMessage =
  | { source: "oxide-artifact"; type: "ready" }
  | { source: "oxide-artifact"; type: "rendered" }
  | { source: "oxide-artifact"; type: "error"; message: string };

/** What the app sends the frame once it's ready. */
export interface RenderMessage {
  source: "oxide";
  type: "render";
  kind: "html" | "react";
  content: string;
}

export function isFrameMessage(data: unknown): data is FrameMessage {
  return (
    typeof data === "object" &&
    data !== null &&
    (data as { source?: unknown }).source === "oxide-artifact" &&
    typeof (data as { type?: unknown }).type === "string"
  );
}
