import { ChevronRight, Loader2 } from "lucide-react";
import { useParams } from "react-router";
import type { ToolCallView } from "@/features/chat/streams";
import { cn } from "@/lib/cn";
import { KindIcon } from "./KindIcon";
import { kindLine } from "./kinds";
import { artifactCallInfo, artifactRefOf } from "./partial";
import { useArtifactPanel } from "./store";

/** An `artifact` tool call in the thread: a card that opens the artifact in the panel. */
export function ArtifactCard({ call }: { call: ToolCallView }) {
  const { id: conversationId = "" } = useParams();
  const info = artifactCallInfo(call.inputJson);
  const ref = artifactRefOf(call.result);
  const open = useArtifactPanel((s) => s.open);
  const openLive = useArtifactPanel((s) => s.openLive);
  const showing = useArtifactPanel(
    (s) =>
      s.target !== null &&
      (s.target.liveCallId === call.id ||
        (ref !== null &&
          s.target.artifactId === ref.artifactId &&
          (s.target.version ?? ref.version) === ref.version)),
  );
  const busy = call.status !== "done" && call.status !== "error";
  const title = ref?.title || info.title || (busy ? "Writing an artifact" : "Artifact");
  const verb =
    info.command === "update" ? "Edited" : info.command === "rewrite" ? "Rewrote" : "Created";

  const onClick = () => {
    if (ref) open(conversationId, ref.artifactId, ref.version);
    else openLive(conversationId, call.id, true);
  };

  return (
    <button
      type="button"
      data-tool-call={call.id}
      onClick={onClick}
      aria-label={`Open artifact ${title}`}
      className={cn(
        "group flex w-full max-w-md items-center gap-3 rounded-xl border bg-raised p-2.5 text-left shadow-sm transition-colors",
        showing ? "border-accent" : "border-line hover:border-line-strong",
      )}
    >
      <span className="flex size-10 shrink-0 items-center justify-center rounded-lg bg-sunken text-fg-muted">
        <KindIcon kind={info.kind} className="size-5" />
      </span>
      <span className="min-w-0 flex-1">
        <span className="block truncate text-md font-medium text-fg">{title}</span>
        <span className="flex items-center gap-1.5 truncate text-sm text-fg-subtle">
          {busy ? (
            <>
              <Loader2 aria-hidden className="size-3.5 animate-spin" />
              {info.command === "update" ? "Editing…" : "Writing…"}
            </>
          ) : (
            <>
              {kindLine(info.kind, info.language)}
              {ref && ref.version > 1 ? ` · ${verb} version ${ref.version}` : ""}
            </>
          )}
        </span>
      </span>
      <ChevronRight
        aria-hidden
        className="size-4 shrink-0 text-fg-subtle transition-transform group-hover:translate-x-0.5"
      />
    </button>
  );
}
