import { Check, ChevronRight, Copy, Pencil, RotateCcw } from "lucide-react";
import { useEffect, useRef, useState, type ReactNode } from "react";
import type { Message } from "@/bindings";
import { IconButton } from "@/components/ui/IconButton";
import { cn } from "@/lib/cn";
import { messageText, segments, thinkingOf, type ThinkingInfo } from "./thread";

/** Text → paragraphs and code blocks. Phase 4 replaces this with react-markdown + Shiki + KaTeX. */
function Prose({ text }: { text: string }) {
  return (
    <>
      {segments(text).map((seg, i) =>
        seg.kind === "p" ? (
          <p key={i} className="whitespace-pre-wrap">
            {seg.text}
          </p>
        ) : (
          <div key={i} className="overflow-hidden rounded-lg border border-line bg-code">
            <div className="flex h-8 items-center border-b border-line px-3 font-sans text-xs text-fg-subtle">
              {seg.lang}
            </div>
            <pre className="!rounded-none !border-0 px-4 py-3">
              <code>{seg.code}</code>
            </pre>
          </div>
        ),
      )}
    </>
  );
}

function useCopy(): [boolean, (text: string) => void] {
  const [copied, setCopied] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(
    () => () => {
      if (timer.current) clearTimeout(timer.current);
    },
    [],
  );
  const copy = (text: string) => {
    navigator.clipboard?.writeText(text).catch(() => {});
    setCopied(true);
    if (timer.current) clearTimeout(timer.current);
    timer.current = setTimeout(() => setCopied(false), 1500);
  };
  return [copied, copy];
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

export function UserMessage({
  message,
  onEdit,
}: {
  message: Message;
  onEdit: (text: string) => void;
}) {
  const [copied, copy] = useCopy();
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState("");
  const text = messageText(message);

  if (editing) {
    return (
      <div className="flex justify-end">
        <div className="w-full max-w-[85%] rounded-2xl border border-line-strong bg-raised p-3 shadow-sm">
          <textarea
            aria-label="Edit message"
            autoFocus
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
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
              disabled={!draft.trim()}
              onClick={() => {
                onEdit(draft.trim());
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
      <div className="max-w-[85%] select-text whitespace-pre-wrap rounded-2xl bg-bubble px-4 py-2.5 text-md leading-6 text-fg">
        {text}
      </div>
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
  );
}

export function AssistantMessage({
  message,
  isLast,
  onRetry,
}: {
  message: Message;
  isLast: boolean;
  onRetry?: () => void;
}) {
  const [copied, copy] = useCopy();
  const thinking = thinkingOf(message);
  return (
    <div className="group flex flex-col gap-2">
      {thinking && <ThinkingBlock thinking={thinking} />}
      <div className="ox-prose select-text">
        <Prose text={messageText(message)} />
      </div>
      <Actions visible={isLast}>
        <IconButton
          size="sm"
          label={copied ? "Copied" : "Copy"}
          onClick={() => copy(messageText(message))}
        >
          {copied ? <Check className="size-4" /> : <Copy className="size-4" />}
        </IconButton>
        {onRetry && (
          <IconButton size="sm" label="Retry" onClick={onRetry}>
            <RotateCcw className="size-4" />
          </IconButton>
        )}
      </Actions>
    </div>
  );
}

function WorkingDots() {
  return (
    <span aria-hidden className="inline-flex items-center gap-[3px] text-accent">
      <span className="ox-dot size-[5px] rounded-full bg-current" />
      <span className="ox-dot size-[5px] rounded-full bg-current" />
    </span>
  );
}

/** Collapsed "Thought for Ns" line; expands to show the reasoning text. */
export function ThinkingBlock({ thinking }: { thinking: ThinkingInfo }) {
  const [open, setOpen] = useState(false);
  return (
    <div>
      <button
        type="button"
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
        className="inline-flex items-center gap-2 rounded-md py-1 text-md text-fg-muted hover:text-fg"
      >
        <span aria-hidden className="inline-flex items-center gap-[3px] text-accent">
          <span className="size-[5px] rounded-full bg-current opacity-80" />
          <span className="size-[5px] rounded-full bg-current opacity-50" />
        </span>
        Thought for {thinking.seconds}s
        <ChevronRight className={cn("size-4 transition-transform", open && "rotate-90")} />
      </button>
      {open && (
        <div className="ml-[5px] mt-1 select-text border-l-2 border-line pl-4 text-base leading-6 text-fg-muted">
          {thinking.text}
        </div>
      )}
    </div>
  );
}

/** Live status line while a reply is being generated. */
export function WorkingLine({ startedAt }: { startedAt: number }) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const t = setInterval(() => setNow(Date.now()), 500);
    return () => clearInterval(t);
  }, []);
  const secs = Math.max(0, Math.floor((now - startedAt) / 1000));
  return (
    <div role="status" className="inline-flex items-center gap-2 py-1 text-md text-fg-muted">
      <WorkingDots />
      Thinking
      <span className="text-fg-subtle tabular-nums">{secs}s</span>
    </div>
  );
}
