import { AlertCircle, ArrowDown, RotateCcw } from "lucide-react";
import {
  Fragment,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { Link, useParams } from "react-router";
import type { Attachment, Compaction, Feedback, Message } from "@/bindings";
import { ArtifactPanel } from "@/features/artifacts/ArtifactPanel";
import {
  useConversation,
  useEditAndResend,
  useMessages,
  useRegenerate,
  useSendMessage,
  useSetConversationModel,
  useSetFeedback,
  useStopGeneration,
} from "@/features/chats/queries";
import { Composer } from "@/features/composer/Composer";
import { CompactionDivider } from "@/features/context/CompactionDivider";
import { useCompactions } from "@/features/context/queries";
import { useGenerateOptions } from "@/features/composer/options";
import { usePendingApprovals } from "@/features/tools/queries";
import { ApprovalCard } from "@/features/tools/ToolCallBlock";
import { SessionPanel } from "@/features/workspace/SessionPanel";
import { useSessionPanel } from "@/features/workspace/store";
import { errorMessage } from "@/lib/api";
import { useBranches } from "./branches";
import { AssistantMessage, LiveReplyView, UserMessage, type Versions } from "./Messages";
import { useStreams } from "./streams";
import { activeThread, newestLeafUnder, siblingsOf } from "./thread";

const NO_COMPACTIONS: Compaction[] = [];

/** Distance from the bottom (px) within which the view follows a streaming reply. */
const STICK_DISTANCE = 120;
/** Show "Jump to latest" once the user is this far from the bottom. */
const JUMP_DISTANCE = 400;

export function ChatPage() {
  const { id = "" } = useParams();
  // Keyed by id so the draft and scroll position reset when switching chats.
  return <ChatView key={id} id={id} />;
}

/**
 * Keeps a scroll container pinned to the bottom while its content grows (streaming text,
 * images loading, code highlighting), unless the user has scrolled up to read.
 */
function useStickToBottom() {
  const scrollRef = useRef<HTMLDivElement>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  const stick = useRef(true);
  const [showJump, setShowJump] = useState(false);

  const toBottom = useCallback((smooth = false) => {
    const el = scrollRef.current;
    if (!el) return;
    if (smooth && typeof el.scrollTo === "function") {
      el.scrollTo({ top: el.scrollHeight, behavior: "smooth" });
    } else {
      el.scrollTop = el.scrollHeight;
    }
  }, []);

  useEffect(() => {
    const content = contentRef.current;
    if (!content || typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(() => {
      if (stick.current) toBottom();
    });
    ro.observe(content);
    return () => ro.disconnect();
  }, [toBottom]);

  const onScroll = useCallback(() => {
    const el = scrollRef.current;
    if (!el) return;
    const distance = el.scrollHeight - el.scrollTop - el.clientHeight;
    stick.current = distance < STICK_DISTANCE;
    setShowJump(distance > JUMP_DISTANCE);
  }, []);

  /** Follow the bottom again (after sending, or "Jump to latest"). */
  const follow = useCallback(
    (smooth = false) => {
      stick.current = true;
      setShowJump(false);
      toBottom(smooth);
    },
    [toBottom],
  );

  return { scrollRef, contentRef, stick, showJump, onScroll, follow, toBottom };
}

function ChatView({ id }: { id: string }) {
  const { conversation: convo, isPending: listPending } = useConversation(id);
  const messagesQuery = useMessages(convo ? id : "");
  // Only the reply's identity, so this view doesn't re-render on every streamed delta.
  const liveParent = useStreams((s) => s.live[id]?.parentId);
  const isLive = liveParent !== undefined;
  const failure = useStreams((s) => s.failures[id]);
  const clearFailure = useStreams((s) => s.clearFailure);
  const pickedLeaf = useBranches((s) => s.leaf[id]);
  const setLeaf = useBranches((s) => s.setLeaf);
  const clearLeaf = useBranches((s) => s.clear);
  const send = useSendMessage();
  const edit = useEditAndResend();
  const regenerate = useRegenerate();
  const stop = useStopGeneration();
  const setModel = useSetConversationModel();
  const rate = useSetFeedback();
  const options = useGenerateOptions();
  const panel = useSessionPanel(id);
  const compactions = useCompactions(convo ? id : "");
  const [draft, setDraft] = useState("");
  const scroll = useStickToBottom();

  const messages = messagesQuery.data;
  const thread = useMemo(
    () => activeThread(messages ?? [], liveParent ?? pickedLeaf),
    [messages, liveParent, pickedLeaf],
  );
  const leaf = thread[thread.length - 1];

  // Where the transcript shows "Conversation compacted": after a message, or inside a reply.
  const dividers = useMemo(() => {
    const after = new Map<string, Compaction[]>();
    const inside = new Map<string, Compaction[]>();
    for (const c of compactions) {
      const map = c.afterToolUseId ? inside : after;
      map.set(c.messageId, [...(map.get(c.messageId) ?? []), c]);
    }
    return { after, inside };
  }, [compactions]);

  const versions = useMemo(() => {
    const out = new Map<string, Versions>();
    for (const m of thread) {
      const sibs = siblingsOf(messages ?? [], m);
      if (sibs.length > 1) {
        out.set(m.id, { index: sibs.findIndex((s) => s.id === m.id), count: sibs.length });
      }
    }
    return out;
  }, [thread, messages]);

  // Handlers read the latest values through a ref, so they stay stable and memoized
  // messages don't re-render when unrelated state changes.
  const latest = useRef({ messages, options, id });
  useLayoutEffect(() => {
    latest.current = { messages, options, id };
  });
  const { follow } = scroll;
  const handlers = useMemo(
    () => ({
      selectVersion: (m: Message, index: number) => {
        const all = latest.current.messages ?? [];
        const target = siblingsOf(all, m)[index];
        if (target) setLeaf(latest.current.id, newestLeafUnder(all, target.id));
      },
      // After a send, edit or retry the newest branch (the one just written) is shown.
      retry: (m: Message) => {
        const { id: cid, options: o } = latest.current;
        clearFailure(cid);
        follow();
        regenerate.mutate({ messageId: m.id, options: o }, { onSuccess: () => clearLeaf(cid) });
      },
      edit: (m: Message, text: string) => {
        const { id: cid, options: o } = latest.current;
        follow();
        edit.mutate({ messageId: m.id, text, options: o }, { onSuccess: () => clearLeaf(cid) });
      },
      rate: (m: Message, feedback: Feedback | null) => rate.mutate({ message: m, feedback }),
    }),
    // The mutation objects' `mutate` functions are stable.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [setLeaf, clearFailure, clearLeaf, follow],
  );

  // Scroll to the bottom when the thread changes (new messages, another branch) and the
  // view is following.
  const { stick, toBottom } = scroll;
  useEffect(() => {
    if (stick.current) toBottom();
  }, [thread.length, failure, stick, toBottom]);

  if (listPending) return <div className="flex-1" />;

  if (!convo) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center gap-3 text-center">
        <p className="font-serif text-2xl text-fg">This chat doesn’t exist</p>
        <p className="text-fg-muted">It may have been deleted.</p>
        <Link to="/new" className="text-accent hover:underline">
          Start a new chat
        </Link>
      </div>
    );
  }

  let lastAssistantIdx = -1;
  thread.forEach((m, i) => {
    if (m.role === "assistant") lastAssistantIdx = i;
  });
  const actionError = send.error ?? edit.error ?? regenerate.error ?? setModel.error;
  const retryLeaf = () => {
    if (leaf) handlers.retry(leaf);
  };
  const unanswered =
    !isLive && !failure && leaf?.role === "user" && !send.isPending && !regenerate.isPending;

  return (
    <div className="flex min-h-0 flex-1">
      <div className="relative flex min-w-0 flex-1 flex-col">
        <div
          ref={scroll.scrollRef}
          onScroll={scroll.onScroll}
          className="min-h-0 flex-1 overflow-y-auto"
        >
          <div
            ref={scroll.contentRef}
            role="log"
            aria-label="Messages"
            aria-live="off"
            className="mx-auto flex w-full max-w-thread flex-col gap-6 px-6 pb-10 pt-8"
          >
            {messagesQuery.error && (
              <p role="alert" className="text-danger">
                Couldn’t load messages: {errorMessage(messagesQuery.error)}
              </p>
            )}
            {thread.map((m, i) => (
              <Fragment key={m.id}>
                {m.role === "user" ? (
                  <UserMessage
                    message={m}
                    versions={versions.get(m.id) ?? null}
                    onSelectVersion={handlers.selectVersion}
                    onEdit={handlers.edit}
                  />
                ) : (
                  <AssistantMessage
                    message={m}
                    isLast={!isLive && i === lastAssistantIdx}
                    canRetry={!isLive}
                    versions={versions.get(m.id) ?? null}
                    onSelectVersion={handlers.selectVersion}
                    onRetry={handlers.retry}
                    onRate={handlers.rate}
                    compactions={dividers.inside.get(m.id) ?? NO_COMPACTIONS}
                  />
                )}
                {dividers.after.get(m.id)?.map((c) => (
                  <CompactionDivider key={c.id} compaction={c} />
                ))}
              </Fragment>
            ))}
            <LiveReplyView conversationId={id} />
            {!isLive && <OrphanApprovals conversationId={id} />}
            {!isLive && failure && leaf && (
              <ReplyFailed error={failure.error} onRetry={retryLeaf} />
            )}
            {unanswered && leaf && <Unanswered since={leaf.createdAt} onAnswer={retryLeaf} />}
            {actionError && (
              <p role="alert" className="text-danger">
                {errorMessage(actionError)}
              </p>
            )}
          </div>
        </div>
        <p className="sr-only" role="status" aria-live="polite">
          {isLive ? "Oxide is writing a reply." : ""}
        </p>
        {scroll.showJump && (
          <div className="pointer-events-none absolute inset-x-0 bottom-28 flex justify-center">
            <button
              type="button"
              onClick={() => scroll.follow(true)}
              className="ox-pop pointer-events-auto inline-flex h-8 items-center gap-1.5 rounded-full border border-line bg-raised px-3 text-sm text-fg-muted shadow-md hover:text-fg"
            >
              <ArrowDown className="size-4" />
              Jump to latest
            </button>
          </div>
        )}
        <div className="mx-auto w-full max-w-thread px-6 pb-3">
          <Composer
            variant="reply"
            conversationId={id}
            leafId={leaf?.id ?? null}
            value={draft}
            onChange={setDraft}
            autoFocus
            streaming={isLive}
            onStop={() => stop.mutate(id)}
            modelId={convo.model}
            onModelChange={(model) => setModel.mutate({ id, model })}
            onSubmit={(text: string, attachments: Attachment[]) => {
              scroll.follow();
              send.mutate(
                { conversationId: id, parentId: leaf?.id ?? null, text, attachments, options },
                {
                  onSuccess: () => {
                    setDraft("");
                    clearLeaf(id);
                  },
                },
              );
            }}
          />
        </div>
      </div>
      <ArtifactPanel conversationId={id} />
      {panel.open && <SessionPanel id={id} modelId={convo.model} thread={thread} />}
    </div>
  );
}

function ReplyFailed({ error, onRetry }: { error: string; onRetry: () => void }) {
  return (
    <div
      role="alert"
      className="flex items-start gap-3 rounded-xl border border-line bg-raised px-4 py-3 text-base"
    >
      <AlertCircle className="mt-0.5 size-4 shrink-0 text-danger" />
      <p className="min-w-0 flex-1 text-fg">{error}</p>
      <button
        type="button"
        onClick={onRetry}
        className="inline-flex h-7 shrink-0 items-center gap-1.5 rounded-lg px-2 text-sm text-fg-muted hover:bg-hover hover:text-fg"
      >
        <RotateCcw className="size-3.5" />
        Retry
      </button>
    </div>
  );
}

/**
 * A user message without a reply that isn't being written (the app closed mid-reply, or the
 * reply was stopped before any output). Shown after a moment, so it doesn't flash while a
 * reply is starting.
 */
function Unanswered({ since, onAnswer }: { since: number; onAnswer: () => void }) {
  const [visible, setVisible] = useState(() => Date.now() - since > 1500);
  useEffect(() => {
    if (visible) return;
    const t = setTimeout(() => setVisible(true), 1500);
    return () => clearTimeout(t);
  }, [visible]);
  if (!visible) return null;
  return (
    <p role="note" className="flex items-center gap-2 text-md text-fg-subtle">
      No reply yet.
      <button type="button" onClick={onAnswer} className="text-accent hover:underline">
        Answer this message
      </button>
    </p>
  );
}

/**
 * Tool calls still waiting for approval in a chat whose live reply this window didn't see
 * (it was reloaded while the reply ran). Answering lets the reply continue.
 */
function OrphanApprovals({ conversationId }: { conversationId: string }) {
  const { data } = usePendingApprovals(true);
  const mine = (data ?? []).filter((r) => r.conversationId === conversationId);
  if (mine.length === 0) return null;
  return (
    <div className="flex flex-col gap-2">
      {mine.map((r) => (
        <div
          key={r.toolUseId}
          className="overflow-hidden rounded-xl border border-accent bg-raised"
        >
          <ApprovalCard request={r} />
        </div>
      ))}
    </div>
  );
}
