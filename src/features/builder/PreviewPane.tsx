import { TriangleAlert } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  commands,
  events,
  type AppDraft,
  type PreviewConsoleLine,
  type PreviewReport,
  type PreviewRequestKind,
} from "@/bindings";
import { AppFrame, type FrameControl, type FrameEvents } from "@/features/apps/AppFrame";

/** Console lines kept for the model. */
const CONSOLE_LINES = 50;
/** How long a check waits for the app to render (or fail). */
const RENDER_WAIT_MS = 10_000;
/** After rendering, how long a check keeps listening for runtime errors. */
const SETTLE_MS = 1_200;

/** What one run of the preview reported. */
export interface PreviewLog {
  loaded: boolean;
  rendered: boolean;
  errors: string[];
  console: PreviewConsoleLine[];
}

const emptyLog = (): PreviewLog => ({ loaded: false, rendered: false, errors: [], console: [] });

const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

const wakeAll = (waiting: Set<() => void>) => {
  for (const w of [...waiting]) w();
};

/**
 * The builder's Preview: runs the draft (SDK calls as `draft:<id>`, with the app's grants),
 * reloads when its files change, and answers the builder model's `app_preview_status` and
 * `app_screenshot` (`events.appPreviewRequest`).
 */
export function PreviewPane({
  draft,
  filesKey,
  onFixError,
}: {
  draft: AppDraft;
  /** Changes when the draft's files change (the preview reloads). */
  filesKey: number;
  onFixError: (message: string) => void;
}) {
  const id = draft.app.manifest.id;
  const [forced, setForced] = useState(0);
  const control = useRef<FrameControl | null>(null);
  const log = useRef<PreviewLog>(emptyLog());
  // Wakes checks waiting for the app to render or fail.
  const waiters = useRef(new Set<() => void>());
  const bundle = useMemo(() => ({ app: draft.app, files: draft.files }), [draft.app, draft.files]);
  const problem = draft.app.problem;
  const problemRef = useRef(problem);
  problemRef.current = problem;

  const frameEvents = useMemo<FrameEvents>(
    () => ({
      onLoad: () => {
        log.current = { ...emptyLog(), loaded: true };
      },
      onRendered: () => {
        log.current.rendered = true;
        wakeAll(waiters.current);
      },
      onError: (message) => {
        if (!log.current.errors.includes(message)) log.current.errors.push(message);
        wakeAll(waiters.current);
      },
      onConsole: (level, text) => {
        const lines = log.current.console;
        lines.push({ level, text });
        if (lines.length > CONSOLE_LINES) lines.splice(0, lines.length - CONSOLE_LINES);
      },
    }),
    [],
  );

  /** Reloads the preview and reports what happened. */
  const check = useCallback(async (kind: PreviewRequestKind): Promise<PreviewReport> => {
    if (problemRef.current) {
      return {
        rendered: false,
        errors: [`The app can't run: ${problemRef.current}`],
        console: [],
        screenshot: null,
        note: null,
      };
    }
    log.current = emptyLog();
    setForced((n) => n + 1);
    const started = Date.now();
    while (Date.now() - started < RENDER_WAIT_MS) {
      const l = log.current;
      if (l.loaded && (l.rendered || l.errors.length > 0)) break;
      await new Promise<void>((resolve) => {
        const done = () => {
          waiters.current.delete(done);
          resolve();
        };
        waiters.current.add(done);
        setTimeout(done, 250);
      });
    }
    await sleep(SETTLE_MS);
    const l = log.current;
    const report: PreviewReport = {
      rendered: l.rendered && l.errors.length === 0,
      errors: [...l.errors],
      console: [...l.console],
      screenshot: null,
      note: l.loaded ? null : "The preview didn't start.",
    };
    if (kind === "screenshot") {
      try {
        const shot = control.current ? await control.current.snapshot() : null;
        if (shot) report.screenshot = shot;
        else report.note = "The preview isn't running, so there is no screenshot.";
      } catch (e) {
        report.note = `The screenshot failed: ${e instanceof Error ? e.message : String(e)}`;
      }
    }
    return report;
  }, []);

  // The builder's model asks; one check at a time.
  useEffect(() => {
    let closed = false;
    let unlisten: (() => void) | undefined;
    let queue: Promise<unknown> = Promise.resolve();
    void events.appPreviewRequest
      .listen(({ payload }) => {
        if (payload.appId !== id) return;
        queue = queue.then(async () => {
          const report = await check(payload.kind);
          await commands.answerPreviewRequest(payload.requestId, report).catch(() => {});
        });
      })
      .then((u) => {
        if (closed) u();
        else unlisten = u;
      });
    return () => {
      closed = true;
      unlisten?.();
    };
  }, [id, check]);

  if (problem) {
    return (
      <div className="flex flex-1 items-start justify-center p-8">
        <div className="flex max-w-lg gap-3 rounded-xl border border-line bg-raised p-4 text-base">
          <TriangleAlert className="mt-0.5 size-5 shrink-0 text-danger" />
          <div>
            <p className="font-medium text-fg">The app can't run</p>
            <p className="mt-1 text-fg-muted">{problem}</p>
            <button
              type="button"
              className="mt-3 text-sm text-accent hover:underline"
              onClick={() => onFixError(`The app can't run: ${problem}`)}
            >
              Fix with AI
            </button>
          </div>
        </div>
      </div>
    );
  }

  return (
    <AppFrame
      bundle={bundle}
      reloadKey={filesKey + forced}
      appRef={`draft:${id}`}
      events={frameEvents}
      control={control}
      onFixError={onFixError}
    />
  );
}
