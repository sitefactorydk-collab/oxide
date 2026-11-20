import { ShieldQuestion, Sparkles, TriangleAlert, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState, type MutableRefObject } from "react";
import { events, type AppBundle, type AppCapability } from "@/bindings";
import { useResolvedTheme } from "@/app/useThemeSync";
import { isFrameToHost, type ConsoleLevel, type HostToFrame } from "@/appframe/protocol";
import { APP_FRAME_SANDBOX, APP_FRAME_URL } from "./frameUrl";
import { createAppHost } from "./host";
import { CAPABILITY_TEXT } from "./queries";

interface Question {
  capability: AppCapability;
  message: string;
  answer: (allowed: boolean) => void;
}

/** What the frame reports (the builder's Preview collects it for its model). */
export interface FrameEvents {
  /** The app's files were sent to a fresh frame. */
  onLoad?: () => void;
  /** The app compiled and its first render ran. */
  onRendered?: () => void;
  /** A compile error or an uncaught runtime error. */
  onError?: (message: string) => void;
  onConsole?: (level: ConsoleLevel, text: string) => void;
}

/** Asks things of the running frame. */
export interface FrameControl {
  /** A PNG `data:` URL of the app as shown. */
  snapshot: () => Promise<string>;
}

/** Runs one app in its sandboxed frame and answers its SDK calls. */
export function AppFrame({
  bundle,
  reloadKey,
  appRef,
  events: frameEvents,
  control,
  onFixError,
}: {
  bundle: AppBundle;
  reloadKey: number;
  /** The id SDK calls use (the builder's Preview passes `draft:<id>`). */
  appRef?: string;
  events?: FrameEvents;
  control?: MutableRefObject<FrameControl | null>;
  /** Offered on the error bar as **Fix with AI**. */
  onFixError?: (message: string) => void;
}) {
  const frame = useRef<HTMLIFrameElement>(null);
  const eventsRef = useRef(frameEvents);
  eventsRef.current = frameEvents;
  const theme = useResolvedTheme();
  const themeRef = useRef(theme);
  themeRef.current = theme;
  const [questions, setQuestions] = useState<Question[]>([]);
  const [error, setError] = useState<string | null>(null);
  const app = bundle.app.manifest;
  const callId = appRef ?? app.id;
  const instance = useMemo(
    () => `${app.id}-${reloadKey}-${Math.random().toString(36).slice(2, 8)}`,
    [app.id, reloadKey],
  );

  useEffect(() => {
    setError(null);
    setQuestions([]);
    const send = (m: HostToFrame) => frame.current?.contentWindow?.postMessage(m, "*");
    const host = createAppHost(
      callId,
      (capability, message) =>
        new Promise<boolean>((resolve) =>
          setQuestions((q) => [
            ...q,
            {
              capability,
              message,
              answer: (allowed) => {
                setQuestions((all) => all.filter((x) => x.capability !== capability));
                resolve(allowed);
              },
            },
          ]),
        ),
      instance,
    );
    // Streamed text and progress of running calls.
    const live = new Map<string, string>();
    // Snapshots asked of this frame, by request id.
    const shots = new Map<string, { resolve: (url: string) => void; reject: (e: Error) => void }>();
    let shotCount = 0;
    if (control) {
      control.current = {
        snapshot: () =>
          new Promise<string>((resolve, reject) => {
            const id = `s${++shotCount}`;
            shots.set(id, { resolve, reject });
            send({ source: "oxide-host", type: "snapshot", id });
            setTimeout(() => {
              if (shots.delete(id)) reject(new Error("the frame didn't answer"));
            }, 15000);
          }),
      };
    }
    let unlisten: (() => void) | undefined;
    let closed = false;
    void events.appCallEvent
      .listen((e) => {
        const id = live.get(e.payload.callId);
        if (!id) return;
        let event: unknown;
        try {
          event = JSON.parse(e.payload.eventJson);
        } catch {
          return;
        }
        send({ source: "oxide-host", type: "event", id, event });
      })
      .then((u) => {
        if (closed) u();
        else unlisten = u;
      });

    const onMessage = (e: MessageEvent) => {
      if (!frame.current || e.source !== frame.current.contentWindow) return;
      if (!isFrameToHost(e.data)) return;
      const m = e.data;
      switch (m.type) {
        case "ready":
          send({
            source: "oxide-host",
            type: "load",
            app: { id: app.id, name: app.name },
            entry: app.entry ?? "App.tsx",
            files: bundle.files,
            theme: themeRef.current,
          });
          eventsRef.current?.onLoad?.();
          break;
        case "error":
          setError(m.message);
          eventsRef.current?.onError?.(m.message);
          break;
        case "rendered":
          eventsRef.current?.onRendered?.();
          break;
        case "console":
          eventsRef.current?.onConsole?.(m.level, m.text);
          break;
        case "snapshot": {
          const waiting = shots.get(m.id);
          shots.delete(m.id);
          if (waiting && m.dataUrl) waiting.resolve(m.dataUrl);
          else waiting?.reject(new Error(m.error ?? "no picture"));
          break;
        }
        case "call": {
          live.set(host.backendId(m.id), m.id);
          host
            .call(m.id, m.method, m.params)
            .then(
              (value) => send({ source: "oxide-host", type: "result", id: m.id, ok: true, value }),
              (error) => send({ source: "oxide-host", type: "result", id: m.id, ok: false, error }),
            )
            .finally(() => live.delete(host.backendId(m.id)));
          break;
        }
        case "cancel":
          host.cancel(m.id);
          break;
      }
    };
    window.addEventListener("message", onMessage);
    return () => {
      closed = true;
      unlisten?.();
      for (const s of shots.values()) s.reject(new Error("the preview reloaded"));
      if (control?.current) control.current = null;
      window.removeEventListener("message", onMessage);
      // Calls still running are stopped with the frame.
      for (const id of live.keys()) host.cancel(id.slice(instance.length + 1));
    };
  }, [callId, app.id, app.name, app.entry, bundle.files, instance, control]);

  useEffect(() => {
    frame.current?.contentWindow?.postMessage(
      { source: "oxide-host", type: "theme", theme } satisfies HostToFrame,
      "*",
    );
  }, [theme]);

  const question = questions[0];
  return (
    <div className="relative flex min-h-0 flex-1 flex-col">
      {question && (
        <div
          role="alertdialog"
          aria-label={`${app.name} asks for permission`}
          className="absolute inset-x-0 top-3 z-20 mx-auto flex w-[min(560px,calc(100%-24px))] flex-col gap-3 rounded-xl border border-line bg-app p-4 shadow-lg"
        >
          <div className="flex items-start gap-3">
            <ShieldQuestion className="mt-0.5 size-5 shrink-0 text-accent" />
            <div className="min-w-0 text-base text-fg">
              <p className="font-medium">
                {app.name} wants to {CAPABILITY_TEXT[question.capability].ask}.
              </p>
              <p className="mt-1 text-sm text-fg-muted">
                Allowed once, it can do this whenever it runs. You can take it back in the app's
                settings.
              </p>
            </div>
          </div>
          <div className="flex justify-end gap-2">
            <button
              type="button"
              onClick={() => question.answer(false)}
              className="h-8 rounded-lg border border-line px-3 text-sm text-fg hover:bg-hover"
            >
              Deny
            </button>
            <button
              type="button"
              autoFocus
              onClick={() => question.answer(true)}
              className="h-8 rounded-lg bg-fg px-3 text-sm font-medium text-app hover:opacity-90"
            >
              Allow for this app
            </button>
          </div>
        </div>
      )}
      {error && (
        <div className="flex items-start gap-2 border-b border-line bg-sunken px-4 py-2 text-sm text-fg-muted">
          <TriangleAlert className="mt-0.5 size-4 shrink-0 text-danger" />
          <span className="min-w-0 flex-1 break-words">The app reported an error: {error}</span>
          {onFixError && (
            <button
              type="button"
              onClick={() => onFixError(error)}
              className="inline-flex shrink-0 items-center gap-1 rounded-md px-1.5 text-accent hover:bg-hover"
            >
              <Sparkles className="size-3.5" /> Fix with AI
            </button>
          )}
          <button type="button" aria-label="Dismiss" onClick={() => setError(null)}>
            <X className="size-4" />
          </button>
        </div>
      )}
      <iframe
        key={instance}
        ref={frame}
        title={app.name}
        src={APP_FRAME_URL}
        sandbox={APP_FRAME_SANDBOX}
        className="min-h-0 w-full flex-1 border-0 bg-app"
      />
    </div>
  );
}
