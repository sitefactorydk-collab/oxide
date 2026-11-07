import {
  AlertTriangle,
  ChevronRight,
  FileText,
  FolderSearch,
  Globe,
  Loader2,
  Plug,
  ShieldAlert,
  Shapes,
  Sparkles,
  ShieldCheck,
} from "lucide-react";
import { useState } from "react";
import type { ApprovalDecision, ContentBlock, ToolApprovalRequest } from "@/bindings";
import { ArtifactCard } from "@/features/artifacts/ArtifactCard";
import { ImageThumb } from "@/features/attachments/MessageAttachments";
import { useStreams, type ToolCallView } from "@/features/chat/streams";
import { useMcpUi } from "@/features/mcp/store";
import { errorMessage } from "@/lib/api";
import { cn } from "@/lib/cn";
import { useAnswerApproval } from "./queries";
import { callHeadline, prettyJson, toolKind } from "./toolView";

function KindIcon({ call }: { call: ToolCallView }) {
  const kind = toolKind(call);
  const Icon =
    kind === "search"
      ? Globe
      : kind === "fetch"
        ? FileText
        : kind === "project"
          ? FolderSearch
          : kind === "artifact"
            ? Shapes
            : kind === "skill"
              ? Sparkles
              : Plug;
  return <Icon aria-hidden className="size-4 shrink-0" />;
}

function StatusBadge({ call }: { call: ToolCallView }) {
  const progress = useMcpUi((s) => s.progress[call.id]);
  switch (call.status) {
    case "streaming":
    case "ready":
    case "running":
      return (
        <span className="inline-flex shrink-0 items-center gap-1.5 text-sm text-fg-subtle">
          <Loader2 aria-hidden className="size-3.5 animate-spin" />
          {call.status === "running" && progress
            ? `${progress.message ?? "Working"}${progress.total ? ` ${Math.round((progress.progress / progress.total) * 100)}%` : ""}`
            : call.status === "running"
              ? "Running"
              : "Preparing"}
        </span>
      );
    case "awaitingApproval":
      return <span className="shrink-0 text-sm text-accent">Needs your approval</span>;
    case "approved":
      return <span className="shrink-0 text-sm text-fg-subtle">Allowed · starting</span>;
    case "error":
      return (
        <span className="inline-flex shrink-0 items-center gap-1 text-sm text-danger">
          <AlertTriangle aria-hidden className="size-3.5" />
          {call.result ? "Failed" : "Didn’t run"}
        </span>
      );
    case "done":
      return call.durationMs !== null && call.durationMs >= 1000 ? (
        <span className="shrink-0 text-sm tabular-nums text-fg-subtle">
          {(call.durationMs / 1000).toFixed(1)}s
        </span>
      ) : null;
  }
}

/** What a tool returned: text, images. */
export function ToolResultView({
  content,
  isError,
}: {
  content: ContentBlock[];
  isError: boolean;
}) {
  let imageIndex = 0;
  return (
    <div className="flex flex-col gap-2">
      {content.map((b, i) => {
        if (b.type === "text") {
          return (
            <pre
              key={i}
              className={cn(
                "max-h-72 overflow-auto whitespace-pre-wrap break-words rounded-lg bg-sunken px-3 py-2 font-mono text-xs leading-5",
                isError ? "text-danger" : "text-fg-muted",
              )}
            >
              {b.text}
            </pre>
          );
        }
        if (b.type === "image") {
          const n = imageIndex++;
          return b.source.kind === "path" ? (
            <ImageThumb key={i} path={b.source.path} index={n} />
          ) : (
            <img
              key={i}
              alt={`Tool image ${n + 1}`}
              src={`data:${b.mediaType};base64,${b.source.data}`}
              className="max-h-60 max-w-full rounded-lg border border-line object-contain"
            />
          );
        }
        return null;
      })}
    </div>
  );
}

/** "Allow Oxide to use …?" with the arguments and three answers. */
export function ApprovalCard({ request }: { request: ToolApprovalRequest }) {
  const answer = useAnswerApproval();
  const name = request.title ?? request.tool;
  const send = (decision: ApprovalDecision) => {
    // Close the card at once; the call starts when the backend hears the answer.
    const streams = useStreams.getState();
    streams.markAnswered(request.conversationId, request.toolUseId, decision !== "deny");
    if (decision === "allowChat") {
      streams.markToolAllowed(request.conversationId, request.server, request.tool);
    }
    answer.mutate({
      conversationId: request.conversationId,
      toolUseId: request.toolUseId,
      decision,
    });
  };
  const Icon = request.destructive ? ShieldAlert : ShieldCheck;
  return (
    <div
      role="group"
      aria-label={`Approve ${name}`}
      className="flex flex-col gap-3 border-t border-line px-3 pb-3 pt-3"
    >
      <div className="flex items-start gap-2.5">
        <Icon
          aria-hidden
          className={cn(
            "mt-0.5 size-4 shrink-0",
            request.destructive ? "text-danger" : "text-accent",
          )}
        />
        <div className="min-w-0 text-md">
          <p className="text-fg">
            Allow Oxide to use <strong className="font-semibold">{name}</strong>
            {request.server ? (
              <>
                {" "}
                from <strong className="font-semibold">{request.server}</strong>
              </>
            ) : null}
            ?
          </p>
          {request.description && (
            <p className="mt-0.5 line-clamp-3 text-sm text-fg-subtle">{request.description}</p>
          )}
          {request.destructive && (
            <p className="mt-1 text-sm text-danger">
              The server says this tool can change or delete data.
            </p>
          )}
        </div>
      </div>
      <pre
        aria-label="Arguments"
        className="max-h-56 overflow-auto rounded-lg bg-sunken px-3 py-2 font-mono text-xs leading-5 text-fg-muted"
      >
        {prettyJson(request.inputJson)}
      </pre>
      <div className="flex flex-wrap justify-end gap-2">
        <button
          type="button"
          disabled={answer.isPending}
          onClick={() => send("deny")}
          className="h-8 rounded-lg px-3 text-sm text-fg-muted hover:bg-hover hover:text-fg disabled:opacity-50"
        >
          Deny
        </button>
        <button
          type="button"
          disabled={answer.isPending}
          onClick={() => send("allowChat")}
          className="h-8 rounded-lg border border-line-strong px-3 text-sm text-fg hover:bg-hover disabled:opacity-50"
        >
          Allow always for this chat
        </button>
        <button
          type="button"
          autoFocus
          disabled={answer.isPending}
          onClick={() => send("allowOnce")}
          className="h-8 rounded-lg bg-fg px-3 text-sm font-medium text-app disabled:opacity-50"
        >
          Allow once
        </button>
      </div>
      {answer.error && (
        <p role="alert" className="text-sm text-danger">
          {errorMessage(answer.error)}
        </p>
      )}
    </div>
  );
}

/**
 * A tool call in the thread. Artifacts show as a card that opens the artifact panel (failed
 * ones as a normal line, so the error can be read).
 */
export function ToolCallBlock({ call }: { call: ToolCallView }) {
  if (toolKind(call) === "artifact" && call.status !== "error") {
    return <ArtifactCard call={call} />;
  }
  return <ToolLine call={call} />;
}

/** One line that opens to show the arguments and the result. */
function ToolLine({ call }: { call: ToolCallView }) {
  const [open, setOpen] = useState(false);
  const { action, detail } = callHeadline(call);
  const waiting = call.status === "awaitingApproval" && call.approval !== null;
  return (
    <div
      data-tool-call={call.id}
      className={cn(
        "overflow-hidden rounded-xl border bg-raised",
        waiting ? "border-accent shadow-sm" : "border-line",
      )}
    >
      <button
        type="button"
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
        className="flex w-full min-w-0 items-center gap-2 px-3 py-2 text-left text-md text-fg-muted hover:text-fg"
      >
        <KindIcon call={call} />
        <span className="shrink-0 text-fg">{action}</span>
        {detail && <span className="min-w-0 truncate text-fg-subtle">{detail}</span>}
        {call.server && toolKind(call) === "connector" && (
          <span className="shrink-0 rounded bg-sunken px-1.5 py-0.5 font-mono text-[11px] text-fg-subtle">
            {call.server}
          </span>
        )}
        <span className="ml-auto" />
        <StatusBadge call={call} />
        <ChevronRight
          aria-hidden
          className={cn("size-4 shrink-0 transition-transform", open && "rotate-90")}
        />
      </button>
      {open && (
        <div className="flex flex-col gap-2 border-t border-line px-3 py-3">
          {call.inputJson.trim() !== "" && call.inputJson.trim() !== "{}" && (
            <>
              <div className="text-xs font-medium uppercase tracking-wide text-fg-subtle">
                Request
              </div>
              <pre className="max-h-56 overflow-auto rounded-lg bg-sunken px-3 py-2 font-mono text-xs leading-5 text-fg-muted">
                {prettyJson(call.inputJson)}
              </pre>
            </>
          )}
          {call.result && (
            <>
              <div className="text-xs font-medium uppercase tracking-wide text-fg-subtle">
                {call.isError ? "Error" : "Response"}
              </div>
              <ToolResultView content={call.result} isError={call.isError} />
            </>
          )}
        </div>
      )}
      {waiting && call.approval && <ApprovalCard request={call.approval} />}
    </div>
  );
}
