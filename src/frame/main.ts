/**
 * The artifact frame: runs one HTML page or React component sent by the app.
 *
 * It's loaded from Oxide's `oxide-artifact` protocol into an `<iframe>` sandboxed without
 * `allow-same-origin`, so it has an opaque origin and no way into the app; its CSP (set by
 * the protocol) blocks network requests. The app posts `{ source: "oxide", type: "render" }`
 * after the frame says it's ready; the frame renders once and reports errors back.
 */
import * as React from "react";
import * as ReactDOM from "react-dom";
import * as ReactDOMClient from "react-dom/client";
import * as Lucide from "lucide-react";
import { ArtifactError, compile } from "./run";

// Nothing of the host should be reachable from artifact code, even if a webview injected it
// into this frame.
const hostGlobals = ["__TAURI__", "__TAURI_INTERNALS__", "__TAURI_IPC__", "__TAURI_METADATA__"];
for (const name of hostGlobals) {
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
        // read-only: nothing more to do
      }
    }
  }
}

type Out = { type: "ready" } | { type: "rendered" } | { type: "error"; message: string };

function post(msg: Out): void {
  window.parent.postMessage({ source: "oxide-artifact", ...msg }, "*");
}

function message(e: unknown): string {
  if (e instanceof Error) return e.message;
  return String(e);
}

window.addEventListener("error", (e) => post({ type: "error", message: e.message }));
window.addEventListener("unhandledrejection", (e) =>
  post({ type: "error", message: message(e.reason) }),
);

function renderHtml(content: string): void {
  post({ type: "rendered" });
  // Replaces this document (and this script's listeners) with the artifact's page.
  document.open();
  document.write(content);
  document.close();
}

class Boundary extends React.Component<{ children: React.ReactNode }, { error: Error | null }> {
  state: { error: Error | null } = { error: null };
  static getDerivedStateFromError(error: Error) {
    return { error };
  }
  componentDidCatch(error: Error) {
    post({ type: "error", message: error.message });
  }
  render() {
    if (this.state.error) {
      return React.createElement(
        "pre",
        {
          style: {
            margin: 16,
            padding: 12,
            whiteSpace: "pre-wrap",
            font: "12px/1.5 ui-monospace, monospace",
            color: "#b42318",
            background: "#fef3f2",
            borderRadius: 8,
          },
        },
        this.state.error.message,
      );
    }
    return this.props.children;
  }
}

function showError(text: string): void {
  const root = document.getElementById("root");
  if (root) {
    const pre = document.createElement("pre");
    pre.textContent = text;
    pre.setAttribute(
      "style",
      "margin:16px;padding:12px;white-space:pre-wrap;font:12px/1.5 ui-monospace,monospace;color:#b42318;background:#fef3f2;border-radius:8px",
    );
    root.replaceChildren(pre);
  }
  post({ type: "error", message: text });
}

async function renderReact(content: string): Promise<void> {
  try {
    const [babelModule] = await Promise.all([
      import("@babel/standalone"),
      // Generates Tailwind's CSS for the classes the component uses.
      import("@tailwindcss/browser"),
    ]);
    const modules = {
      react: React,
      "react-dom": ReactDOM,
      "react-dom/client": ReactDOMClient,
      "lucide-react": Lucide,
    };
    // A CommonJS package: its functions are on the module or on its default export.
    const transform = babelModule.transform ?? babelModule.default?.transform;
    if (!transform) throw new ArtifactError("The JSX compiler couldn't be loaded.");
    const App = compile(content, transform, modules) as React.ComponentType;
    const root = document.getElementById("root");
    if (!root) throw new ArtifactError("The frame has no root element.");
    ReactDOMClient.createRoot(root).render(
      React.createElement(Boundary, null, React.createElement(App)),
    );
    post({ type: "rendered" });
  } catch (e) {
    showError(message(e));
  }
}

let done = false;
window.addEventListener("message", (e: MessageEvent) => {
  if (done || e.source !== window.parent) return;
  const d = e.data as { source?: unknown; type?: unknown; kind?: unknown; content?: unknown };
  if (d?.source !== "oxide" || d.type !== "render" || typeof d.content !== "string") return;
  done = true;
  if (d.kind === "html") renderHtml(d.content);
  else if (d.kind === "react") void renderReact(d.content);
});

post({ type: "ready" });
