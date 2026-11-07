import {
  Check,
  ChevronLeft,
  ChevronRight,
  Copy,
  Download,
  Loader2,
  Maximize2,
  Minimize2,
  X,
} from "lucide-react";
import { useEffect, useState, type PointerEvent as ReactPointerEvent } from "react";
import type { ArtifactKind } from "@/bindings";
import { IconButton } from "@/components/ui/IconButton";
import { useStreams, type ToolCallView } from "@/features/chat/streams";
import { errorMessage } from "@/lib/api";
import { cn } from "@/lib/cn";
import { toast } from "@/lib/toast";
import { useCopy } from "@/lib/useCopy";
import { KindIcon } from "./KindIcon";
import { hasPreview, kindLine } from "./kinds";
import { artifactCallInfo } from "./partial";
import { useArtifact, useChatArtifacts, useSaveArtifactFile } from "./queries";
import { CHAT_MIN, PANEL_DEFAULT, clampPanelWidth, useArtifactPanel } from "./store";
import { ArtifactView } from "./views/ArtifactView";

/** The live `artifact` call the panel shows, while the reply is being written. */
function useLiveCall(conversationId: string, callId: string | null): ToolCallView | null {
  return useStreams((s) => {
    if (!callId) return null;
    const live = s.live[conversationId];
    const block = live?.blocks.find((b) => b.kind === "tool" && b.call.id === callId);
    return block?.kind === "tool" ? block.call : null;
  });
}

interface Shown {
  kind: ArtifactKind;
  title: string;
  language: string | null;
  content: string;
  /** Saved artifact: its id, the version shown, and how many there are. */
  saved: { id: string; version: number; count: number } | null;
  /** Still being written (live), or being updated by the model. */
  busy: "writing" | "updating" | null;
}

/** Side panel beside the chat: one artifact, its versions, code ↔ preview. */
export function ArtifactPanel({ conversationId }: { conversationId: string }) {
  const target = useArtifactPanel((s) => s.target);
  if (!target || target.conversationId !== conversationId) return null;
  return <Panel conversationId={conversationId} />;
}

function Panel({ conversationId }: { conversationId: string }) {
  const target = useArtifactPanel((s) => s.target);
  const view = useArtifactPanel((s) => s.view);
  const setView = useArtifactPanel((s) => s.setView);
  const fullscreen = useArtifactPanel((s) => s.fullscreen);
  const setFullscreen = useArtifactPanel((s) => s.setFullscreen);
  const close = useArtifactPanel((s) => s.close);
  const setVersion = useArtifactPanel((s) => s.setVersion);
  const width = useArtifactPanel((s) => s.width);

  const liveCall = useLiveCall(conversationId, target?.liveCallId ?? null);
  const info = liveCall ? artifactCallInfo(liveCall.inputJson) : null;
  const { data: chatArtifacts } = useChatArtifacts(conversationId);
  // A live update shows the artifact it changes.
  const updating =
    info && info.content === null && info.key
      ? (chatArtifacts ?? []).find((a) => a.key === info.key)
      : undefined;
  const artifactId = target?.artifactId ?? updating?.id ?? null;
  const detail = useArtifact(artifactId);

  useEffect(() => {
    if (!fullscreen) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setFullscreen(false);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [fullscreen, setFullscreen]);

  let shown: Shown | null = null;
  if (info && info.content !== null) {
    shown = {
      kind: info.kind ?? "code",
      title: info.title || "Untitled",
      language: info.language,
      content: info.content,
      saved: null,
      busy: "writing",
    };
  } else if (detail.data) {
    const versions = detail.data.versions;
    const wanted = target?.liveCallId ? null : (target?.version ?? null);
    const v = versions.find((x) => x.version === wanted) ?? versions[versions.length - 1];
    if (v) {
      shown = {
        kind: v.kind,
        title: v.title,
        language: v.language,
        content: v.content,
        saved: { id: v.artifactId, version: v.version, count: versions.length },
        busy: liveCall ? "updating" : null,
      };
    }
  }

  const preview = shown ? hasPreview(shown.kind) : false;
  const effectiveView = preview ? view : "code";

  return (
    <aside
      aria-label="Artifact"
      style={
        fullscreen ? undefined : { width: `${width}px`, maxWidth: `calc(100% - ${CHAT_MIN}px)` }
      }
      className={cn(
        "flex min-h-0 flex-col border-l border-line bg-raised",
        fullscreen ? "fixed inset-x-0 bottom-0 top-titlebar z-30 shadow-lg" : "relative shrink-0",
      )}
    >
      {!fullscreen && <ResizeHandle />}
      <header className="flex h-12 shrink-0 items-center gap-2 border-b border-line pl-4 pr-2">
        <span className="text-fg-subtle">
          <KindIcon kind={shown?.kind ?? null} />
        </span>
        <div className="min-w-0 flex-1">
          <h2 className="truncate text-md font-medium text-fg">{shown?.title ?? "Artifact"}</h2>
          <p className="flex items-center gap-1.5 truncate text-xs text-fg-subtle">
            {shown ? kindLine(shown.kind, shown.language) : ""}
            {shown?.busy && (
              <>
                <Loader2 aria-hidden className="size-3 animate-spin" />
                {shown.busy === "writing" ? "Writing…" : "Updating…"}
              </>
            )}
          </p>
        </div>
        {shown?.saved && shown.saved.count > 1 && (
          <VersionSwitcher
            version={shown.saved.version}
            count={shown.saved.count}
            onPick={(v) => setVersion(v === shown.saved?.count ? null : v)}
          />
        )}
        {preview && (
          <div role="group" aria-label="View" className="flex rounded-lg bg-sunken p-0.5 text-sm">
            {(["preview", "code"] as const).map((v) => (
              <button
                key={v}
                type="button"
                aria-pressed={effectiveView === v}
                onClick={() => setView(v)}
                className={cn(
                  "h-7 rounded-md px-2.5 capitalize",
                  effectiveView === v
                    ? "bg-raised text-fg shadow-sm"
                    : "text-fg-muted hover:text-fg",
                )}
              >
                {v === "preview" ? "Preview" : "Code"}
              </button>
            ))}
          </div>
        )}
        {shown && <CopyButton text={shown.content} />}
        {shown?.saved && <DownloadButton id={shown.saved.id} version={shown.saved.version} />}
        <IconButton
          size="sm"
          label={fullscreen ? "Exit full screen" : "Full screen"}
          onClick={() => setFullscreen(!fullscreen)}
        >
          {fullscreen ? <Minimize2 className="size-4" /> : <Maximize2 className="size-4" />}
        </IconButton>
        <IconButton size="sm" label="Close artifact" onClick={close}>
          <X className="size-4" />
        </IconButton>
      </header>
      <div className="min-h-0 flex-1 overflow-auto">
        {shown ? (
          <ArtifactView
            kind={shown.kind}
            language={shown.language}
            content={shown.content}
            title={shown.title}
            view={effectiveView}
            streaming={shown.busy === "writing"}
          />
        ) : detail.error ? (
          <p role="alert" className="p-4 text-danger">
            Couldn’t load the artifact: {errorMessage(detail.error)}
          </p>
        ) : target?.liveCallId && !liveCall ? (
          <p className="p-4 text-fg-muted">This artifact wasn’t saved.</p>
        ) : (
          <div className="flex h-full items-center justify-center">
            <Loader2 aria-label="Loading artifact" className="size-5 animate-spin text-fg-subtle" />
          </div>
        )}
      </div>
    </aside>
  );
}

function VersionSwitcher({
  version,
  count,
  onPick,
}: {
  version: number;
  count: number;
  onPick: (v: number) => void;
}) {
  return (
    <div className="flex items-center text-sm text-fg-muted">
      <IconButton
        size="sm"
        label="Previous version"
        disabled={version <= 1}
        onClick={() => onPick(version - 1)}
      >
        <ChevronLeft className="size-4" />
      </IconButton>
      <span aria-live="polite" className="px-1 tabular-nums">
        Version {version} of {count}
      </span>
      <IconButton
        size="sm"
        label="Next version"
        disabled={version >= count}
        onClick={() => onPick(version + 1)}
      >
        <ChevronRight className="size-4" />
      </IconButton>
    </div>
  );
}

function CopyButton({ text }: { text: string }) {
  const [copied, copy] = useCopy();
  return (
    <IconButton size="sm" label={copied ? "Copied" : "Copy"} onClick={() => copy(text)}>
      {copied ? <Check className="size-4" /> : <Copy className="size-4" />}
    </IconButton>
  );
}

function DownloadButton({ id, version }: { id: string; version: number }) {
  const save = useSaveArtifactFile();
  return (
    <IconButton
      size="sm"
      label="Download"
      disabled={save.isPending}
      onClick={() => {
        save
          .mutateAsync({ id, version })
          .then((path) => {
            if (path) toast(`Saved to ${path}`);
          })
          .catch((e: unknown) => toast(`Couldn’t save: ${errorMessage(e)}`));
      }}
    >
      <Download className="size-4" />
    </IconButton>
  );
}

/** Drag the panel's left edge to resize; double-click resets. */
function ResizeHandle() {
  const setWidth = useArtifactPanel((s) => s.setWidth);
  const [dragging, setDragging] = useState(false);

  const onPointerDown = (e: ReactPointerEvent<HTMLDivElement>) => {
    e.preventDefault();
    const startX = e.clientX;
    const startW = useArtifactPanel.getState().width;
    const target = e.currentTarget;
    // The chat column keeps some room.
    const available = target.parentElement?.parentElement?.clientWidth ?? window.innerWidth;
    target.setPointerCapture(e.pointerId);
    setDragging(true);
    const onMove = (ev: PointerEvent) =>
      setWidth(clampPanelWidth(startW - (ev.clientX - startX), available));
    const onUp = () => {
      setDragging(false);
      target.removeEventListener("pointermove", onMove);
      target.removeEventListener("pointerup", onUp);
      target.removeEventListener("pointercancel", onUp);
    };
    target.addEventListener("pointermove", onMove);
    target.addEventListener("pointerup", onUp);
    target.addEventListener("pointercancel", onUp);
  };

  return (
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label="Resize artifact panel"
      onPointerDown={onPointerDown}
      onDoubleClick={() => setWidth(PANEL_DEFAULT)}
      className={cn(
        "absolute -left-[3px] top-0 z-10 h-full w-[6px] cursor-col-resize transition-colors",
        dragging ? "bg-accent/40" : "hover:bg-line-strong",
      )}
    />
  );
}
