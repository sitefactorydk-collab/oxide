import "@fontsource-variable/inter";
import "@fontsource-variable/source-serif-4";
import "@fontsource-variable/jetbrains-mono";
import "katex/dist/katex.min.css";
import "@/styles/index.css";

import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

const root = document.getElementById("root");
if (!root) throw new Error("#root element missing from index.html");

/**
 * `index.html?window=quick` is the quick entry window, `?window=computer` computer use's
 * control bar; anything else is the app.
 */
const windowKind = new URLSearchParams(window.location.search).get("window");
const isQuickEntry = windowKind === "quick";

async function start(el: HTMLElement) {
  if (windowKind === "computer") {
    const { ComputerBarRoot } = await import("@/features/computer/ComputerBar");
    document.documentElement.dataset.window = "computer";
    createRoot(el).render(
      <StrictMode>
        <ComputerBarRoot />
      </StrictMode>,
    );
    return;
  }
  if (isQuickEntry) {
    // Loaded on its own, so the small window doesn't parse the whole app.
    const { QuickEntryRoot } = await import("@/features/quick-entry/QuickEntryRoot");
    document.documentElement.dataset.window = "quick";
    createRoot(el).render(
      <StrictMode>
        <QuickEntryRoot />
      </StrictMode>,
    );
    return;
  }
  const [{ createHashRouter }, { default: App }, { AppLayout }] = await Promise.all([
    import("react-router"),
    import("@/app/App"),
    import("@/app/AppLayout"),
  ]);
  // Hash history: works from Tauri's asset protocol without server-side fallbacks.
  const router = createHashRouter([{ path: "*", element: <AppLayout /> }]);
  createRoot(el).render(
    <StrictMode>
      <App router={router} />
    </StrictMode>,
  );
}

void start(root);
