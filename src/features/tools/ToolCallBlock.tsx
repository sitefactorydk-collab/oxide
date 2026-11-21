import {
  AlertTriangle,
  Box,
  Braces,
  ChevronRight,
  Compass,
  PanelTop,
  FileText,
  FolderOpen,
  FolderSearch,
  Globe,
  ListChecks,
  Loader2,
  MonitorSmartphone,
  Plug,
  SquareTerminal,
  ShieldAlert,
  Shapes,
  Sparkles,
  ShieldCheck,
  Music,
  Film,
  ImageIcon,
} from "lucide-react";
import { useState } from "react";
import { useParams } from "react-router";
import type { ApprovalDecision, ContentBlock, ToolApprovalRequest } from "@/bindings";
import { ArtifactCard } from "@/features/artifacts/ArtifactCard";
import { ComputerApprovalCard } from "@/features/computer/ComputerApprovalCard";
import { SkillSaveDetails } from "@/features/extensions/SkillSavePreview";
import { useSkillFocus } from "@/features/extensions/store";
import { useOpenSettings } from "@/features/settings/useOpenSettings";
import { useBrowserFrame } from "@/features/browser/queries";
import { ImageThumb } from "@/features/attachments/MessageAttachments";
import { GeneratedMedia, MediaPlayer } from "@/features/generation/GeneratedMedia";
import { isPlayable } from "@/features/generation/media";
import { useStreams, type ToolCallView } from "@/features/chat/streams";
import { useArtifactPanel } from "@/features/artifacts/store";
import { useMcpUi } from "@/features/mcp/store";
import { usePythonInstalling } from "@/features/python/queries";
import { errorMessage } from "@/lib/api";
import { cn } from "@/lib/cn";
import { AutoMarkBadge } from "@/features/permissions/AutoMarkBadge";
import { useAnswerApproval, useToolUsage } from "./queries";
import {
  browserGoal,
  callHeadline,
  prettyJson,
  pythonCode,
  sandboxCommand,
  savedSkillFolder,
  toolKind,
} from "./toolView";

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
              : kind === "file"
                ? FolderOpen
                : kind === "tasks"
                  ? ListChecks
                  : kind === "shell"
                    ? SquareTerminal
                    : kind === "python"
                      ? Braces
                      : kind === "sandbox"
                        ? Box
                        : kind === "browser"
                          ? Compass
                          : kind === "chrome"
                            ? PanelTop
                            : kind === "computer"
                              ? MonitorSmartphone
                              : kind === "generation"
                                ? call.tool === "generate_video"
                                  ? Film
                                  : call.tool === "generate_music"
                                    ? Music
                                    : ImageIcon
                                : Plug;
  return <Icon aria-hidden className="size-4 shrink-0" />;
}

function StatusBadge({ call }: { call: ToolCallView }) {
  const progress = useMcpUi((s) => s.progress[call.id]);
  const frame = useBrowserFrame(call.id);
  const isPython = toolKind(call) === "python";
  const settingUp = usePythonInstalling(isPython) && isPython;
  if (call.checking && (call.status === "ready" || call.status === "streaming")) {
    return (
      <span className="inline-flex shrink-0 items-center gap-1.5 text-sm text-fg-subtle">
        <Loader2 aria-hidden className="size-3.5 animate-spin" />
        Safety check
      </span>
    );
  }
  switch (call.status) {
    case "streaming":
    case "ready":
    case "running":
      return (
        <span className="inline-flex shrink-0 items-center gap-1.5 text-sm text-fg-subtle">
          <Loader2 aria-hidden className="size-3.5 animate-spin" />
          {call.status === "running" && settingUp ? (
            "Setting up Python"
          ) : call.status === "running" && frame?.text ? (
            <span className="max-w-48 truncate">{frame.text}</span>
          ) : call.status === "running" && call.progress ? (
            `${call.progress.note}${call.progress.percent !== null ? ` ${call.progress.percent}%` : ""}`
          ) : call.status === "running" && progress ? (
            `${progress.message ?? "Working"}${progress.total ? ` ${Math.round((progress.progress / progress.total) * 100)}%` : ""}`
          ) : call.status === "running" ? (
            "Running"
          ) : (
            "Preparing"
          )}
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
        if (b.type === "artifactRef") {
          return <ArtifactLink key={i} id={b.artifactId} title={b.title} version={b.version} />;
        }
        if (isPlayable(b)) {
          return <MediaPlayer key={i} path={b.source.path} mediaType={b.mediaType} name={b.name} />;
        }
        return null;
      })}
    </div>
  );
}

/** A file a script produced that opened as an artifact: a button that shows it in the panel. */
function ArtifactLink({ id, title, version }: { id: string; title: string; version: number }) {
  const { id: conversationId = "" } = useParams();
  const open = useArtifactPanel((s) => s.open);
  return (
    <button
      type="button"
      aria-label={`Open artifact ${title}`}
      onClick={() => open(conversationId, id, version)}
      className="inline-flex w-fit max-w-full items-center gap-2 rounded-lg border border-line bg-raised px-3 py-1.5 text-left text-sm text-fg hover:border-line-strong"
    >
      <Shapes aria-hidden className="size-4 shrink-0 text-fg-muted" />
      <span className="truncate">{title}</span>
    </button>
  );
}

/** The approval card for a call: computer use has its own. */
export function ApprovalCard({ request }: { request: ToolApprovalRequest }) {
  return request.computer ? (
    <ComputerApprovalCard request={request} computer={request.computer} />
  ) : (
    <ToolApprovalCard request={request} />
  );
}

/** "Allow Oxide to use …?" with the arguments and three answers. */
function ToolApprovalCard({ request }: { request: ToolApprovalRequest }) {
  const answer = useAnswerApproval();
  const name = request.title ?? request.tool;
  const send = (decision: ApprovalDecision) => {
    // Close the card at once; the call starts when the backend hears the answer.
    const streams = useStreams.getState();
    streams.markAnswered(request.conversationId, request.toolUseId, decision !== "deny");
    if (decision === "allowChat") {
      streams.markToolAllowed(
        request.conversationId,
        request.server,
        request.tool,
        request.inputJson,
      );
    }
    answer.mutate({
      conversationId: request.conversationId,
      toolUseId: request.toolUseId,
      decision,
    });
  };
  const Icon = request.destructive ? ShieldAlert : ShieldCheck;
  // A skill shapes later chats: every save is asked about on its own.
  const skill = request.server === null && request.tool === "save_skill" ? request.skill : null;
  const once = request.server === null && request.tool === "save_skill";
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
          {skill ? (
            <p className="text-fg">
              {skill.isNew ? "Save a new skill " : "Change the skill "}
              <strong className="font-mono font-semibold">{skill.name}</strong>?
            </p>
          ) : (
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
          )}
          {request.autoReason && (
            <p className="mt-1 text-sm text-accent">
              <strong className="font-semibold">Auto stopped this.</strong> Stopped because:{" "}
              {request.autoReason}.
            </p>
          )}
          {request.description && !once && (
            <p className="mt-0.5 line-clamp-3 text-sm text-fg-subtle">{request.description}</p>
          )}
          {request.destructive && (
            <p className="mt-1 text-sm text-danger">
              {request.server === "chrome"
                ? "This closes a tab Oxide opened in your Chrome."
                : request.server
                  ? "The server says this tool can change or delete data."
                  : request.tool === "run_command"
                    ? "Commands run on your computer and can change or delete files."
                    : request.tool === "run_python"
                      ? "Python runs on your computer, not in a sandbox. It can read, change or delete your files."
                      : "This changes files in a folder you shared."}
            </p>
          )}
        </div>
      </div>
      {request.server === "browser" && (
        <p className="text-sm text-fg-muted">
          {request.tool === "browser_task"
            ? "An autonomous agent browses on its own until the goal is done, opening pages, clicking and typing as you would."
            : request.tool === "navigate"
              ? "The chat's browser opens this page (or searches Google for these words)."
              : request.tool === "close"
                ? "The chat's browser closes."
                : "The chat's browser acts on the website as you would."}
        </p>
      )}
      {request.server === "chrome" && (
        <p className="text-sm text-fg-muted">
          {request.tool === "navigate" || request.tool === "tabs_create"
            ? "This opens the page in your own Chrome, with your sign-ins (or searches Google for these words)."
            : "This acts in your own Chrome, with your sign-ins, as you would."}{" "}
          The extension shows Stop in the tab.
        </p>
      )}
      {request.server === null && request.tool.startsWith("sandbox_copy") && (
        <p className="text-sm text-fg-muted">
          {request.tool === "sandbox_copy_in"
            ? "This copies from a folder you shared into the chat's sandbox."
            : "This copies from the chat's sandbox into a folder you shared. Nothing there is overwritten."}
        </p>
      )}
      {skill ? (
        <SkillSaveDetails preview={skill} />
      ) : request.server === null && request.tool === "sandbox_exec" ? (
        <pre
          aria-label="Command"
          className="max-h-72 overflow-auto rounded-lg bg-sunken px-3 py-2 font-mono text-xs leading-5 text-fg"
        >
          {sandboxCommand(request.inputJson) || prettyJson(request.inputJson)}
        </pre>
      ) : request.server === "browser" && request.tool === "browser_task" ? (
        <pre
          aria-label="Goal"
          className="max-h-72 overflow-auto whitespace-pre-wrap rounded-lg bg-sunken px-3 py-2 font-sans text-sm leading-5 text-fg"
        >
          {browserGoal(request.inputJson) || prettyJson(request.inputJson)}
        </pre>
      ) : request.server === null && request.tool === "run_python" ? (
        <pre
          aria-label="Code"
          className="max-h-72 overflow-auto rounded-lg bg-sunken px-3 py-2 font-mono text-xs leading-5 text-fg"
        >
          {pythonCode(request.inputJson) || prettyJson(request.inputJson)}
        </pre>
      ) : (
        <pre
          aria-label="Arguments"
          className="max-h-56 overflow-auto rounded-lg bg-sunken px-3 py-2 font-mono text-xs leading-5 text-fg-muted"
        >
          {prettyJson(request.inputJson)}
        </pre>
      )}
      <div className="flex flex-wrap justify-end gap-2">
        <button
          type="button"
          disabled={answer.isPending}
          onClick={() => send("deny")}
          className="h-8 rounded-lg px-3 text-sm text-fg-muted hover:bg-hover hover:text-fg disabled:opacity-50"
        >
          Deny
        </button>
        {!once && (
          <button
            type="button"
            disabled={answer.isPending}
            onClick={() => send("allowChat")}
            className="h-8 rounded-lg border border-line-strong px-3 text-sm text-fg hover:bg-hover disabled:opacity-50"
          >
            {request.server === null && request.tool === "run_command"
              ? "Always allow this command"
              : "Allow always for this chat"}
          </button>
        )}
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

/** "Open in Settings": shows a skill a chat just saved in Settings → Skills. */
function SavedSkillLink({ folder }: { folder: string }) {
  const openSettings = useOpenSettings();
  const focus = useSkillFocus((s) => s.focus);
  if (folder === "") return null;
  return (
    <div className="border-t border-line px-3 py-1.5">
      <button
        type="button"
        onClick={() => {
          focus(folder);
          openSettings("skills");
        }}
        className="inline-flex items-center gap-1.5 text-sm text-accent hover:underline"
      >
        <Sparkles aria-hidden className="size-3.5" />
        Open {folder} in Settings → Skills
      </button>
    </div>
  );
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
        <AutoMarkOf call={call} />
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
                {toolKind(call) === "python"
                  ? pythonCode(call.inputJson) || prettyJson(call.inputJson)
                  : prettyJson(call.inputJson)}
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
      {!open && toolKind(call) === "python" && call.result && !call.isError && (
        <PythonOutputs content={call.result} />
      )}
      {!open && toolKind(call) === "generation" && call.result && !call.isError && (
        <GeneratedMedia content={call.result} />
      )}
      {!open && toolKind(call) === "browser" && <BrowserView call={call} />}
      {!open && toolKind(call) === "computer" && <ComputerShot call={call} />}
      {call.server === null &&
        call.tool === "save_skill" &&
        call.status === "done" &&
        !call.isError && <SavedSkillLink folder={savedSkillFolder(call.inputJson)} />}
      {waiting && call.approval && <ApprovalCard request={call.approval} />}
    </div>
  );
}

/** The call's **Auto** mark: live from the stream, else from the chat's tool log. */
function AutoMarkOf({ call }: { call: ToolCallView }) {
  const { id: conversationId = "" } = useParams();
  const { data } = useToolUsage(call.auto ? "" : conversationId);
  const mark = call.auto ?? data?.find((u) => u.toolUseId === call.id)?.auto ?? null;
  if (!mark || call.status === "awaitingApproval") return null;
  return <AutoMarkBadge mark={mark} />;
}

/**
 * What the browser shows: the live screenshot while a call runs, then the screenshots a task
 * or `screenshot` returned (or, for other calls, the last live screenshot of this session).
 */
function BrowserView({ call }: { call: ToolCallView }) {
  const frame = useBrowserFrame(call.id);
  const running = call.status === "running";
  const images = (call.result ?? []).filter((b) => b.type === "image");
  if (!running && images.length > 0 && !call.isError) {
    return (
      <div className="border-t border-line px-3 py-3">
        <ToolResultView content={images} isError={false} />
      </div>
    );
  }
  if (!frame?.image || (!running && call.isError)) return null;
  return (
    <figure className="flex flex-col gap-1.5 border-t border-line px-3 py-3">
      <img
        alt={running ? "Live view of the browser" : "The browser after this step"}
        src={`data:image/jpeg;base64,${frame.image}`}
        className={cn(
          "w-full rounded-lg border border-line object-contain",
          running ? "max-h-80" : "max-h-40 w-auto self-start",
        )}
      />
      {frame.url && (
        <figcaption className="truncate text-xs text-fg-subtle" title={frame.url}>
          {frame.url}
        </figcaption>
      )}
    </figure>
  );
}

/** The screenshot a computer action came back with, small, under its line. */
function ComputerShot({ call }: { call: ToolCallView }) {
  const shot = (call.result ?? []).filter((b) => b.type === "image").slice(-1);
  if (shot.length === 0 || call.isError) return null;
  return (
    <div className="border-t border-line px-3 py-3 [&_img]:max-h-40">
      <ToolResultView content={shot} isError={false} />
    </div>
  );
}

/** Charts and files a script produced, shown under the call so nobody has to open it. */
function PythonOutputs({ content }: { content: ContentBlock[] }) {
  const shown = content.filter((b) => b.type === "image" || b.type === "artifactRef");
  if (shown.length === 0) return null;
  return (
    <div className="border-t border-line px-3 py-3">
      <ToolResultView content={shown} isError={false} />
    </div>
  );
}
