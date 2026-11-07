import { AlertTriangle, Loader2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { FRAME_SANDBOX, FRAME_URL, isFrameMessage, type RenderMessage } from "./frameUrl";

/**
 * Runs an HTML page or a React component in the sandboxed frame. A new frame is made for
 * every content (the parent keys this view by content), so nothing carries over.
 */
export function FrameView({
  kind,
  content,
  title,
}: {
  kind: "html" | "react";
  content: string;
  title: string;
}) {
  const ref = useRef<HTMLIFrameElement>(null);
  const [state, setState] = useState<"loading" | "rendered" | "error">("loading");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const onMessage = (e: MessageEvent) => {
      const frame = ref.current?.contentWindow;
      if (!frame || e.source !== frame || !isFrameMessage(e.data)) return;
      switch (e.data.type) {
        case "ready": {
          const msg: RenderMessage = { source: "oxide", type: "render", kind, content };
          // The frame's origin is opaque, so "*" is the only target that reaches it; the
          // message holds nothing the artifact doesn't already have.
          frame.postMessage(msg, "*");
          break;
        }
        case "rendered":
          setState((s) => (s === "error" ? s : "rendered"));
          break;
        case "error":
          setState("error");
          setError(e.data.message);
          break;
      }
    };
    window.addEventListener("message", onMessage);
    return () => window.removeEventListener("message", onMessage);
  }, [kind, content]);

  return (
    <div className="relative h-full w-full bg-white">
      <iframe
        ref={ref}
        title={title}
        src={FRAME_URL}
        sandbox={FRAME_SANDBOX}
        referrerPolicy="no-referrer"
        className="block h-full w-full border-0 bg-white"
      />
      {state === "loading" && (
        <div className="pointer-events-none absolute inset-0 flex items-center justify-center">
          <Loader2 aria-label="Loading preview" className="size-5 animate-spin text-fg-subtle" />
        </div>
      )}
      {error && (
        <div
          role="alert"
          className="absolute inset-x-3 bottom-3 flex items-start gap-2 rounded-lg border border-line bg-raised px-3 py-2 text-sm text-fg shadow-md"
        >
          <AlertTriangle aria-hidden className="mt-0.5 size-4 shrink-0 text-danger" />
          <span className="min-w-0 flex-1 whitespace-pre-wrap break-words font-mono text-xs">
            {error}
          </span>
          <button
            type="button"
            onClick={() => setError(null)}
            className="shrink-0 text-xs text-fg-muted hover:text-fg"
          >
            Dismiss
          </button>
        </div>
      )}
    </div>
  );
}
