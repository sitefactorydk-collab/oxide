/**
 * The Utility App frame (Phase 20): runs one app sent by the main window.
 *
 * Loaded from Oxide's `oxide-app` protocol into an `<iframe>` sandboxed without
 * `allow-same-origin`, so the app has an opaque origin and no way into Oxide; the protocol's
 * CSP blocks all network requests. The app reaches Oxide only through the `oxide` SDK, whose
 * calls go to the main window by `postMessage`.
 */
import "@fontsource-variable/inter";
import * as React from "react";
import * as ReactDOM from "react-dom";
import * as ReactDOMClient from "react-dom/client";
import * as Lucide from "lucide-react";
import * as ReactMarkdown from "react-markdown";
import * as RemarkGfm from "remark-gfm";
import type { Transform } from "@/frame/run";
import { AppLoadError, appComponent, loadApp } from "./loader";
import { isHostToFrame, type ConsoleLevel, type FrameToHost, type Theme } from "./protocol";
import { snapshot } from "./snapshot";
import * as sdk from "./sdk";

// Nothing of the host should be reachable from app code, even if a webview injected it.
for (const name of ["__TAURI__", "__TAURI_INTERNALS__", "__TAURI_IPC__", "__TAURI_METADATA__"]) {
  const w = window as unknown as Record<string, unknown>;
  if (!(name in w)) continue;
  try {
    delete w[name];
  } catch {
    // not configurable
  }
  if (name in w) {
    try {
      Object.defineProperty(window, name, { value: undefined, configurable: false });
    } catch {
      try {
        w[name] = undefined;
      } catch {
        // read-only
      }
    }
  }
}

function post(msg: FrameToHost): void {
  window.parent.postMessage(msg, "*");
}

function message(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

/** One console argument as text (objects as JSON, cut). */
function show(v: unknown): string {
  if (typeof v === "string") return v;
  if (v instanceof Error) return v.stack ?? v.message;
  try {
    return JSON.stringify(v) ?? String(v);
  } catch {
    return String(v);
  }
}

// The app's console also goes to the main window (the builder's Preview reports it).
for (const level of ["log", "info", "warn", "error", "debug"] as ConsoleLevel[]) {
  const original = console[level].bind(console);
  console[level] = (...args: unknown[]) => {
    original(...args);
    try {
      const text = args.map(show).join(" ").slice(0, 2000);
      post({ source: "oxide-app", type: "console", level, text });
    } catch {
      // never let logging break the app
    }
  };
}

window.addEventListener("error", (e) =>
  post({ source: "oxide-app", type: "error", message: e.message }),
);
window.addEventListener("unhandledrejection", (e) =>
  post({ source: "oxide-app", type: "error", message: message(e.reason) }),
);

function applyTheme(theme: Theme) {
  document.documentElement.classList.toggle("dark", theme === "dark");
  document.documentElement.style.colorScheme = theme;
  sdk.setTheme(theme);
}

/** A bundled package as a module object that also works with default imports. */
function esm(ns: Record<string, unknown>): Record<string, unknown> {
  return { ...ns, __esModule: true, default: ns.default ?? ns };
}

function showError(text: string) {
  const root = document.getElementById("root");
  if (root) {
    const pre = document.createElement("pre");
    pre.textContent = text;
    pre.className = "oxide-app-error";
    root.replaceChildren(pre);
  }
  post({ source: "oxide-app", type: "error", message: text });
}

class Boundary extends React.Component<{ children: React.ReactNode }, { error: Error | null }> {
  state: { error: Error | null } = { error: null };
  static getDerivedStateFromError(error: Error) {
    return { error };
  }
  componentDidCatch(error: Error) {
    post({ source: "oxide-app", type: "error", message: error.message });
  }
  render() {
    if (this.state.error) {
      return React.createElement("pre", { className: "oxide-app-error" }, this.state.error.message);
    }
    return this.props.children;
  }
}

async function run(
  app: { id: string; name: string },
  entry: string,
  files: Array<{ path: string; content: string }>,
  theme: Theme,
) {
  applyTheme(theme);
  sdk.connect((m) => window.parent.postMessage(m, "*"), { ...app, theme });
  try {
    const [babel, , jszip] = await Promise.all([
      import("@babel/standalone"),
      // Generates Tailwind's CSS for the classes the app uses; `dark:` follows the class.
      import("@tailwindcss/browser"),
      import("jszip"),
    ]);
    const transform = (babel.transform ?? babel.default?.transform) as Transform | undefined;
    if (!transform) throw new AppLoadError("The compiler couldn't be loaded.");
    const modules: Record<string, unknown> = {
      react: esm(React as unknown as Record<string, unknown>),
      "react-dom": esm(ReactDOM as unknown as Record<string, unknown>),
      "react-dom/client": esm(ReactDOMClient as unknown as Record<string, unknown>),
      "lucide-react": esm(Lucide as unknown as Record<string, unknown>),
      "react-markdown": esm(ReactMarkdown as unknown as Record<string, unknown>),
      "remark-gfm": esm(RemarkGfm as unknown as Record<string, unknown>),
      jszip: esm(jszip as unknown as Record<string, unknown>),
      oxide: esm(sdk as unknown as Record<string, unknown>),
    };
    const exports = loadApp(files, entry, {
      transform,
      modules,
      addStyle: (css, path) => {
        const style = document.createElement("style");
        style.dataset.file = path;
        style.textContent = css;
        document.head.appendChild(style);
      },
    });
    const App = appComponent(exports) as React.ComponentType;
    const root = document.getElementById("root");
    if (!root) throw new AppLoadError("The frame has no root element.");
    ReactDOMClient.createRoot(root).render(
      React.createElement(Boundary, null, React.createElement(App)),
    );
    post({ source: "oxide-app", type: "rendered" });
  } catch (e) {
    showError(message(e));
  }
}

let loaded = false;
window.addEventListener("message", (e: MessageEvent) => {
  if (e.source !== window.parent || !isHostToFrame(e.data)) return;
  const d = e.data;
  switch (d.type) {
    case "load":
      if (loaded) return;
      loaded = true;
      void run(d.app, d.entry, d.files, d.theme);
      break;
    case "theme":
      applyTheme(d.theme);
      break;
    case "snapshot":
      snapshot().then(
        (dataUrl) => post({ source: "oxide-app", type: "snapshot", id: d.id, dataUrl }),
        (e: unknown) =>
          post({ source: "oxide-app", type: "snapshot", id: d.id, error: message(e) }),
      );
      break;
    case "result":
    case "event":
      sdk.deliver(d);
      break;
  }
});

post({ source: "oxide-app", type: "ready" });
