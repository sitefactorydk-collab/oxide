import { inTauri, platform } from "@/lib/platform";

/**
 * Where the Utility App frame loads from: Oxide's `oxide-app` protocol (its own origin; on
 * Windows WebView2 serves custom schemes as `http://<scheme>.localhost`). In a plain browser
 * (tests, `npm run dev`) it's the Vite page.
 */
export const APP_FRAME_URL = !inTauri
  ? "/app-frame.html"
  : platform === "windows"
    ? "http://oxide-app.localhost/app-frame.html"
    : "oxide-app://localhost/app-frame.html";

/** No `allow-same-origin` (opaque origin, no IPC) and no downloads (they go through Oxide). */
export const APP_FRAME_SANDBOX = "allow-scripts allow-forms allow-modals";
