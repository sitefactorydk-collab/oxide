import {
  Check,
  ChevronLeft,
  ChevronRight,
  Copy,
  Info,
  Pencil,
  RotateCcw,
  ThumbsDown,
  ThumbsUp,
  Wrench,
} from "lucide-react";
import { memo, useEffect, useState, type ReactNode } from "react";
import type { Feedback, Message, Rating, Usage } from "@/bindings";
import { Markdown } from "@/components/markdown/Markdown";
import { IconButton } from "@/components/ui/IconButton";
import { Tooltip } from "@/components/ui/Tooltip";
import { MessageAttachments } from "@/features/attachments/MessageAttachments";
import { useModelLabel } from "@/features/models/queries";
import { cn } from "@/lib/cn";
import { useCopy } from "@/lib/useCopy";
import { useStreams } from "./streams";
import { attachmentsOf, messageText, thinkingOf, type ThinkingInfo } from "./thread";

/** Position among a message's versions (edits or regenerated replies), when it has several. */
export interface Versions {
  index: number;
  count: number;
}

function Actions({ children, visible }: { children: ReactNode; visible?: boolean }) {
  return (
    <div
      className={cn(
        "flex items-center gap-0.5 transition-opacity",
        visible ? "opacity-100" : "opacity-0 focus-within:opacity-100 group-hover:opacity-100",
      )}
    >
      {children}
    </div>
  );
}

/** ‹ 2 / 3 › between versions of a message. */
function BranchNav({
  versions,
  onSelect,
}: {
  versions: Versions;
  onSelect: (index: number) => void;
}) {
  const { index, count } = versions;
  return (
    <div role="group" aria-label="Versions" className="flex items-center text-sm text-fg-muted">
      <IconButton
        size="sm"
        label="Previous version"
        disabled={index === 0}
        onClick={() => onSelect(index - 1)}
      >
        <ChevronLeft className="size-4" />
      </IconButton>
      <span
        aria-label={`Version ${index + 1} of ${count}`}
        className="min-w-9 text-center tabular-nums"
      >
        {index + 1} / {count}
      </span>
      <IconButton
        size="sm"
        label="Next version"
        disabled={index >= count - 1}
        onClick={() => onSelect(index + 1)}
      >
        <ChevronRight className="size-4" />
      </IconButton>
    </div>
  );
}

const number = new Intl.NumberFormat("en-US");

function usageText(usage: Usage, modelLabel: string | null): string {
  const parts = [
    `${number.format(usage.inputTokens)} input`,
    `${number.format(usage.outputTokens)} output tokens`,
  ];
  if (usage.cacheReadTokens) parts.push(`${number.format(usage.cacheReadTokens)} read from cache`);
  if (usage.cacheWriteTokens) parts.push(`${number.format(usage.cacheWriteTokens)} cached`);
  return (modelLabel ? `${modelLabel} · ` : "") + parts.join(" · ");
}

function UsageInfo({ usage, model }: { usage: Usage; model: string | null }) {
  const label = useModelLabel(model ?? "");
  const text = usageText(usage, model ? label : null);
  return (
    <Tooltip label={<span className="max-w-80 font-normal">{text}</span>}>
      <button
        type="button"
        aria-label={`Token usage: ${text}`}
        className="inline-flex size-7 items-center justify-center rounded-md text-fg-subtle hover:bg-hover hover:text-fg"
      >
        <Info className="size-4" />
      </button>
    </Tooltip>
  );
}

export const UserMessage = memo(function UserMessage({
  message,
  versions,
  onSelectVersion,
  onEdit,
}: {
  message: Message;
  versions: Versions | null;
  onSelectVersion: (message: Message, index: number) => void;
  onEdit: (message: Message, text: string) => void;
}) {
  const [copied, copy] = useCopy();
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState("");
  const text = messageText(message);
  const files = attachmentsOf(message);

  if (editing) {
    const canSave = draft.trim() !== "" || files.length > 0;
    return (
      <div className="flex flex-col items-end gap-2">
        <MessageAttachments blocks={files} />
        <div className="w-full max-w-[85%] rounded-2xl border border-line-strong bg-raised p-3 shadow-sm">
          <textarea
            aria-label="Edit message"
            autoFocus
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape") setEditing(false);
            }}
            rows={Math.min(8, draft.split("\n").length + 1)}
            className="block w-full resize-none bg-transparent text-md leading-6 outline-none"
          />
          <div className="mt-2 flex justify-end gap-2">
            <button
              type="button"
              onClick={() => setEditing(false)}
              className="h-8 rounded-lg px-3 text-sm text-fg-muted hover:bg-hover"
            >
              Cancel
            </button>
            <button
              type="button"
              disabled={!canSave}
              onClick={() => {
                onEdit(message, draft.trim());
                setEditing(false);
              }}
              className="h-8 rounded-lg bg-fg px-3 text-sm font-medium text-app disabled:opacity-40"
            >
              Save
            </button>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="group flex flex-col items-end gap-1">
      <MessageAttachments blocks={files} />
      {text && (
        <div className="max-w-[85%] select-text whitespace-pre-wrap break-words rounded-2xl bg-bubble px-4 py-2.5 text-md leading-6 text-fg">
          {text}
        </div>
      )}
      <div className="flex items-center gap-1">
        {versions && (
          <BranchNav versions={versions} onSelect={(i) => onSelectVersion(message, i)} />
        )}
        <Actions>
          <IconButton size="sm" label={copied ? "Copied" : "Copy"} onClick={() => copy(text)}>
            {copied ? <Check className="size-4" /> : <Copy className="size-4" />}
          </IconButton>
          <IconButton
            size="sm"
            label="Edit"
            onClick={() => {
              setDraft(text);
              setEditing(true);
            }}
          >
            <Pencil className="size-4" />
          </IconButton>
        </Actions>
      </div>
    </div>
  );
});

/** Thumbs up / down. Thumbs down asks (optionally) what went wrong. */
function FeedbackButtons({
  feedback,
  onRate,
}: {
  feedback: Feedback | null;
  onRate: (feedback: Feedback | null) => void;
}) {
  const [asking, setAsking] = useState(false);
  const [note, setNote] = useState("");
  const rated = feedback?.rating ?? null;
  const rate = (rating: Rating, withNote: string | null = null) =>
    onRate({ rating, note: withNote, createdAt: Date.now() });

  return (
    <>
      <IconButton
        size="sm"
        label={rated === "good" ? "Remove rating" : "Good response"}
        aria-pressed={rated === "good"}
        active={rated === "good"}
        onClick={() => {
          setAsking(false);
          if (rated === "good") onRate(null);
          else rate("good");
        }}
      >
        <ThumbsUp className={cn("size-4", rated === "good" && "fill-current")} />
      </IconButton>
      <IconButton
        size="sm"
        label={rated === "bad" ? "Remove rating" : "Bad response"}
        aria-pressed={rated === "bad"}
        active={rated === "bad"}
        onClick={() => {
          if (rated === "bad") {
            setAsking(false);
            onRate(null);
          } else {
            rate("bad");
            setNote("");
            setAsking(true);
          }
        }}
      >
        <ThumbsDown className={cn("size-4", rated === "bad" && "fill-current")} />
      </IconButton>
      {asking && (
        <form
          aria-label="What went wrong"
          className="ox-pop absolute left-0 top-full z-10 mt-1 flex w-80 flex-col gap-2 rounded-xl border border-line bg-raised p-3 shadow-md"
          onSubmit={(e) => {
            e.preventDefault();
            rate("bad", note.trim() || null);
            setAsking(false);
          }}
        >
          <label className="text-sm text-fg-muted" htmlFor="feedback-note">
            What went wrong? (optional, stays on this computer)
          </label>
          <textarea
            id="feedback-note"
            autoFocus
            rows={3}
            value={note}
            onChange={(e) => setNote(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape") setAsking(false);
            }}
            className="w-full resize-none rounded-lg border border-line bg-app px-2.5 py-1.5 text-base outline-none focus:border-accent"
          />
          <div className="flex justify-end gap-2">
            <button
              type="button"
              onClick={() => setAsking(false)}
              className="h-8 rounded-lg px-3 text-sm text-fg-muted hover:bg-hover"
            >
              Skip
            </button>
            <button
              type="submit"
              className="h-8 rounded-lg bg-fg px-3 text-sm font-medium text-app"
            >
              Save
            </button>
          </div>
        </form>
      )}
    </>
  );
}

export const AssistantMessage = memo(function AssistantMessage({
  message,
  isLast,
  canRetry,
  versions,
  onSelectVersion,
  onRetry,
  onRate,
}: {
  message: Message;
  isLast: boolean;
  canRetry: boolean;
  versions: Versions | null;
  onSelectVersion: (message: Message, index: number) => void;
  onRetry: (message: Message) => void;
  onRate: (message: Message, feedback: Feedback | null) => void;
}) {
  const [copied, copy] = useCopy();
  const thinking = thinkingOf(message);
  const note = stopNote(message);
  const tools = message.content.flatMap((b) => (b.type === "toolUse" ? [b] : []));
  const text = messageText(message);
  return (
    <article aria-label="Reply" className="group flex flex-col gap-2">
      {thinking && <ThinkingBlock thinking={thinking} />}
      {text && <Markdown text={text} className="ox-prose select-text" />}
      {tools.length > 0 && <ToolCalls names={tools.map((t) => t.name)} />}
      {note && (
        <p role="note" className="text-sm text-fg-subtle">
          {note}
        </p>
      )}
      <div className="relative flex items-center gap-1">
        {versions && (
          <BranchNav versions={versions} onSelect={(i) => onSelectVersion(message, i)} />
        )}
        <Actions visible={isLast || message.feedback !== null}>
          <IconButton
            size="sm"
            label={copied ? "Copied" : "Copy"}
            tooltip
            onClick={() => copy(text)}
          >
            {copied ? <Check className="size-4" /> : <Copy className="size-4" />}
          </IconButton>
          {canRetry && (
            <IconButton size="sm" label="Retry" onClick={() => onRetry(message)}>
              <RotateCcw className="size-4" />
            </IconButton>
          )}
          <FeedbackButtons
            feedback={message.feedback}
            onRate={(feedback) => onRate(message, feedback)}
          />
          {message.usage && <UsageInfo usage={message.usage} model={message.model} />}
        </Actions>
      </div>
    </article>
  );
});

function WorkingDots() {
  return (
    <span aria-hidden className="inline-flex items-center gap-[3px] text-accent">
      <span className="ox-dot size-[5px] rounded-full bg-current" />
      <span className="ox-dot size-[5px] rounded-full bg-current" />
    </span>
  );
}

/** A short note under a reply that didn't end normally. */
function stopNote(m: Message): string | null {
  switch (m.stopReason) {
    case "stopped":
      return "Stopped.";
    case "error":
      return "This reply was cut off by an error.";
    case "maxTokens":
      return "The reply reached the output limit.";
    case "refusal":
      return "The model declined to answer.";
    case "contextFull":
      return "The conversation is too long for this model.";
    case "toolUse":
      return "The model asked to use a tool. Oxide doesn’t run tools yet.";
    default:
      return messageText(m).trim() === "" && !thinkingOf(m) ? "No response." : null;
  }
}

function ToolCalls({ names }: { names: string[] }) {
  return (
    <ul aria-label="Tool calls" className="flex flex-wrap gap-2">
      {names.map((name, i) => (
        <li
          key={`${name}-${i}`}
          className="inline-flex h-7 items-center gap-1.5 rounded-md border border-line bg-sunken px-2 font-mono text-xs text-fg-muted"
        >
          <Wrench className="size-3.5" />
          {name}
        </li>
      ))}
    </ul>
  );
}

/**
 * The reply being written in `conversationId`: live thinking, the text so far, tool calls,
 * a status line. It subscribes to the stream store itself, so the rest of the thread doesn't
 * re-render on every delta.
 */
export function LiveReplyView({ conversationId }: { conversationId: string }) {
  const live = useStreams((s) => s.live[conversationId]);
  if (!live) return null;
  const thinkingDone = live.thinkingEndedAt !== null;
  const thinkingSecs =
    live.thinkingStartedAt !== null && live.thinkingEndedAt !== null
      ? Math.max(1, Math.round((live.thinkingEndedAt - live.thinkingStartedAt) / 1000))
      : 0;
  return (
    <div aria-busy="true" aria-label="Reply in progress" className="flex flex-col gap-2">
      {live.thinking &&
        (thinkingDone ? (
          <ThinkingBlock thinking={{ seconds: thinkingSecs, text: live.thinking }} />
        ) : (
          <ThinkingBlock
            thinking={{ seconds: 0, text: live.thinking }}
            liveSince={live.thinkingStartedAt ?? live.startedAt}
          />
        ))}
      {live.text && <Markdown text={live.text} streaming className="ox-prose select-text" />}
      {live.tools.length > 0 && <ToolCalls names={live.tools.map((t) => t.name)} />}
      {!live.text && !live.thinking && <WorkingLine startedAt={live.startedAt} />}
    </div>
  );
}

/**
 * Collapsed "Thought for Ns" line; expands to show the reasoning text. With `liveSince`, the
 * model is still thinking: the line counts up and the text grows.
 */
export function ThinkingBlock({
  thinking,
  liveSince,
}: {
  thinking: ThinkingInfo;
  liveSince?: number;
}) {
  const [open, setOpen] = useState(false);
  const live = liveSince !== undefined;
  return (
    <div>
      <button
        type="button"
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
        className="inline-flex items-center gap-2 rounded-md py-1 text-md text-fg-muted hover:text-fg"
      >
        {live ? (
          <WorkingDots />
        ) : (
          <span aria-hidden className="inline-flex items-center gap-[3px] text-accent">
            <span className="size-[5px] rounded-full bg-current opacity-80" />
            <span className="size-[5px] rounded-full bg-current opacity-50" />
          </span>
        )}
        {live ? (
          <>
            Thinking
            <Elapsed since={liveSince} />
          </>
        ) : (
          <>Thought for {thinking.seconds}s</>
        )}
        <ChevronRight className={cn("size-4 transition-transform", open && "rotate-90")} />
      </button>
      {open && (
        <div className="ml-[5px] mt-1 select-text whitespace-pre-wrap border-l-2 border-line pl-4 text-base leading-6 text-fg-muted">
          {thinking.text}
        </div>
      )}
    </div>
  );
}

function Elapsed({ since }: { since: number }) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const t = setInterval(() => setNow(Date.now()), 500);
    return () => clearInterval(t);
  }, []);
  const secs = Math.max(0, Math.floor((now - since) / 1000));
  return <span className="tabular-nums text-fg-subtle">{secs}s</span>;
}

/** Live status line while a reply is being generated, before any text arrives. */
export function WorkingLine({ startedAt }: { startedAt: number }) {
  return (
    <div role="status" className="inline-flex items-center gap-2 py-1 text-md text-fg-muted">
      <WorkingDots />
      Working
      <Elapsed since={startedAt} />
    </div>
  );
}
