import {
  AlertCircle,
  ArrowDown,
  FileText,
  FolderSearch,
  Globe,
  Lightbulb,
  Plug,
  RotateCcw,
  Sparkles,
  X,
} from "lucide-react";
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { Link, useParams } from "react-router";
import type { Attachment, Feedback, Message } from "@/bindings";
import { IconButton } from "@/components/ui/IconButton";
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
import { useGenerateOptions } from "@/features/composer/options";
import { EFFORT_LABELS, effectiveEffort, findModel } from "@/features/models/models";
import { useModelLabel, useModels } from "@/features/models/queries";
import { useToolSwitches } from "@/features/mcp/chatTools";
import { useMcpServers } from "@/features/mcp/queries";
import { usePendingApprovals, useToolUsage } from "@/features/tools/queries";
import { ApprovalCard } from "@/features/tools/ToolCallBlock";
import { usageRows, type UsageRow } from "@/features/tools/toolView";
import { errorMessage } from "@/lib/api";
import { useUi } from "@/stores/ui";
import { useBranches } from "./branches";
import { AssistantMessage, LiveReplyView, UserMessage, type Versions } from "./Messages";
import { useStreams } from "./streams";
import { activeThread, attachmentsOf, newestLeafUnder, siblingsOf } from "./thread";

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
  const rightPanelOpen = useUi((s) => s.rightPanelOpen);
  const [draft, setDraft] = useState("");
  const scroll = useStickToBottom();

  const messages = messagesQuery.data;
  const thread = useMemo(
    () => activeThread(messages ?? [], liveParent ?? pickedLeaf),
    [messages, liveParent, pickedLeaf],
  );
  const leaf = thread[thread.length - 1];

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
            {thread.map((m, i) =>
              m.role === "user" ? (
                <UserMessage
                  key={m.id}
                  message={m}
                  versions={versions.get(m.id) ?? null}
                  onSelectVersion={handlers.selectVersion}
                  onEdit={handlers.edit}
                />
              ) : (
                <AssistantMessage
                  key={m.id}
                  message={m}
                  isLast={!isLive && i === lastAssistantIdx}
                  canRetry={!isLive}
                  versions={versions.get(m.id) ?? null}
                  onSelectVersion={handlers.selectVersion}
                  onRetry={handlers.retry}
                  onRate={handlers.rate}
                />
              ),
            )}
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
      {rightPanelOpen && <ChatDetailsPanel id={id} modelId={convo.model} thread={thread} />}
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

function UsageIcon({ kind }: { kind: UsageRow["kind"] }) {
  const Icon =
    kind === "webSearch"
      ? Globe
      : kind === "webFetch"
        ? FileText
        : kind === "providerTool"
          ? Sparkles
          : kind === "projectSearch"
            ? FolderSearch
            : Plug;
  return <Icon aria-hidden className="size-4" />;
}

/** "Used in this chat": web searches, pages, connectors. Click a row to see the call. */
function UsedInChat({ id }: { id: string }) {
  const { data } = useToolUsage(id);
  const rows = usageRows(data ?? []);
  if (rows.length === 0) {
    return <p className="text-sm text-fg-subtle">Nothing yet.</p>;
  }
  return (
    <ul aria-label="Used in this chat" className="flex flex-col gap-1">
      {rows.map((r) => (
        <li key={r.key}>
          <button
            type="button"
            onClick={() =>
              document
                .querySelector(`[data-tool-call="${CSS.escape(r.toolUseId)}"]`)
                ?.scrollIntoView({ behavior: "smooth", block: "center" })
            }
            className="flex w-full items-start gap-2.5 rounded-lg px-2 py-1.5 text-left hover:bg-hover"
          >
            <span className="mt-0.5 text-fg-subtle">
              <UsageIcon kind={r.kind} />
            </span>
            <span className="min-w-0 flex-1">
              <span className="flex items-center gap-1.5 text-base text-fg">
                <span className="truncate">{r.title}</span>
                {r.count > 1 && (
                  <span className="shrink-0 rounded bg-sunken px-1.5 text-xs tabular-nums text-fg-subtle">
                    {r.count}
                  </span>
                )}
                {r.errors > 0 && (
                  <span className="shrink-0 text-xs text-danger">{r.errors} failed</span>
                )}
              </span>
              {r.details.length > 0 && (
                <span className="block truncate text-xs text-fg-subtle">
                  {r.details.slice(-3).join(" · ")}
                </span>
              )}
            </span>
          </button>
        </li>
      ))}
    </ul>
  );
}

/** Which tools the chat may use: web and running connectors. */
function useToolsSummary(id: string): string {
  const switches = useToolSwitches(id);
  const { data: servers } = useMcpServers();
  const on = servers.filter((s) => s.status === "running" && switches.serverOn(s.name));
  const parts: string[] = [];
  if (switches.webOn) parts.push("Web");
  if (on.length > 0) parts.push(on.map((s) => s.name).join(", "));
  return parts.length > 0 ? parts.join(" · ") : "None";
}

/** Right-hand "in this chat" panel: model, tools, files, and what the chat used. */
function ChatDetailsPanel({
  id,
  modelId,
  thread,
}: {
  id: string;
  modelId: string;
  thread: Message[];
}) {
  const toolsSummary = useToolsSummary(id);
  const toggle = useUi((s) => s.toggleRightPanel);
  const thinking = useUi((s) => s.thinking);
  const effort = useUi((s) => s.effort);
  const modelLabel = useModelLabel(modelId);
  const { data: lists } = useModels();
  const caps = findModel(lists, modelId)?.capabilities;
  const applied = caps ? effectiveEffort(caps, effort) : null;
  const files = thread.flatMap((m) =>
    attachmentsOf(m).map((b) => (b.type === "document" ? b.name : "Image")),
  );
  return (
    <aside aria-label="Chat details" className="w-72 shrink-0 p-3">
      <div className="rounded-xl border border-line bg-raised p-4 shadow-sm">
        <div className="mb-3 flex items-center justify-between">
          <h2 className="text-md font-semibold text-fg">In this chat</h2>
          <IconButton size="sm" label="Close details" onClick={toggle}>
            <X className="size-4" />
          </IconButton>
        </div>
        <dl className="flex flex-col gap-3 text-base">
          <Row icon={<Lightbulb className="size-4" />} label="Model">
            {modelLabel}
            {thinking ? " · thinking" : ""}
            {applied ? ` · ${EFFORT_LABELS[applied]} effort` : ""}
          </Row>
          <Row icon={<Plug className="size-4" />} label="Tools">
            {toolsSummary}
          </Row>
          <Row icon={<FileText className="size-4" />} label="Files">
            {files.length === 0 ? "None yet" : `${files.length}: ${files.join(", ")}`}
          </Row>
        </dl>
        <h3 className="mb-1.5 mt-5 text-sm font-semibold text-fg">Used in this chat</h3>
        <UsedInChat id={id} />
      </div>
    </aside>
  );
}

function Row({ icon, label, children }: { icon: ReactNode; label: string; children: ReactNode }) {
  return (
    <div className="flex items-start gap-3">
      <span className="mt-0.5 text-fg-subtle">{icon}</span>
      <div className="min-w-0">
        <dt className="text-xs text-fg-subtle">{label}</dt>
        <dd className="truncate text-fg">{children}</dd>
      </div>
    </div>
  );
}
