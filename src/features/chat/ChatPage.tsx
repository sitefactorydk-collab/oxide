import { AlertCircle, FileText, Lightbulb, Plug, RotateCcw, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { Link, useParams } from "react-router";
import type { GenerateOptions } from "@/bindings";
import { IconButton } from "@/components/ui/IconButton";
import {
  useConversation,
  useEditAndResend,
  useMessages,
  useRegenerate,
  useSendMessage,
  useSetConversationModel,
  useStopGeneration,
} from "@/features/chats/queries";
import { Composer } from "@/features/composer/Composer";
import { useModelLabel } from "@/features/models/queries";
import { errorMessage } from "@/lib/api";
import { useUi } from "@/stores/ui";
import { AssistantMessage, LiveReplyView, UserMessage } from "./Messages";
import { useStreams } from "./streams";
import { activeThread } from "./thread";

/** Distance from the bottom (px) within which the view follows a streaming reply. */
const STICK_DISTANCE = 120;

export function ChatPage() {
  const { id = "" } = useParams();
  // Keyed by id so the draft and scroll position reset when switching chats.
  return <ChatView key={id} id={id} />;
}

function ChatView({ id }: { id: string }) {
  const { conversation: convo, isPending: listPending } = useConversation(id);
  const messagesQuery = useMessages(convo ? id : "");
  const live = useStreams((s) => s.live[id]);
  const failure = useStreams((s) => s.failures[id]);
  const clearFailure = useStreams((s) => s.clearFailure);
  const send = useSendMessage();
  const edit = useEditAndResend();
  const regenerate = useRegenerate();
  const stop = useStopGeneration();
  const setModel = useSetConversationModel();
  const thinking = useUi((s) => s.thinking);
  const rightPanelOpen = useUi((s) => s.rightPanelOpen);
  const [draft, setDraft] = useState("");
  const scrollRef = useRef<HTMLDivElement>(null);
  const stick = useRef(true);

  const options: GenerateOptions = { thinking };
  const messages = messagesQuery.data;
  const liveParent = live?.parentId;
  const thread = useMemo(() => activeThread(messages ?? [], liveParent), [messages, liveParent]);
  const leaf = thread[thread.length - 1];

  // Follow the bottom while new content arrives, unless the user scrolled up to read.
  useEffect(() => {
    const el = scrollRef.current;
    if (el && stick.current) el.scrollTop = el.scrollHeight;
  }, [thread.length, live?.text, live?.thinking, failure, id]);

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
  const retry = (messageId: string) => {
    clearFailure(id);
    stick.current = true;
    regenerate.mutate({ messageId, options });
  };
  const unanswered =
    !live && !failure && leaf?.role === "user" && !send.isPending && !regenerate.isPending;

  return (
    <div className="flex min-h-0 flex-1">
      <div className="flex min-w-0 flex-1 flex-col">
        <div
          ref={scrollRef}
          onScroll={(e) => {
            const el = e.currentTarget;
            stick.current = el.scrollHeight - el.scrollTop - el.clientHeight < STICK_DISTANCE;
          }}
          className="min-h-0 flex-1 overflow-y-auto"
        >
          <div className="mx-auto flex w-full max-w-thread flex-col gap-6 px-6 pb-10 pt-8">
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
                  onEdit={(text) => {
                    stick.current = true;
                    edit.mutate({ messageId: m.id, text, options });
                  }}
                />
              ) : (
                <AssistantMessage
                  key={m.id}
                  message={m}
                  isLast={!live && i === lastAssistantIdx}
                  onRetry={live ? undefined : () => retry(m.id)}
                />
              ),
            )}
            {live && <LiveReplyView live={live} />}
            {!live && failure && leaf && (
              <ReplyFailed error={failure.error} onRetry={() => retry(leaf.id)} />
            )}
            {unanswered && leaf && (
              <Unanswered since={leaf.createdAt} onAnswer={() => retry(leaf.id)} />
            )}
            {actionError && (
              <p role="alert" className="text-danger">
                {errorMessage(actionError)}
              </p>
            )}
          </div>
        </div>
        <div className="mx-auto w-full max-w-thread px-6 pb-3">
          <Composer
            variant="reply"
            value={draft}
            onChange={setDraft}
            autoFocus
            streaming={!!live}
            onStop={() => stop.mutate(id)}
            modelId={convo.model}
            onModelChange={(model) => setModel.mutate({ id, model })}
            onSubmit={(text) => {
              const body = text.trim() || "(attachment)";
              stick.current = true;
              send.mutate(
                { conversationId: id, parentId: leaf?.id ?? null, text: body, options },
                { onSuccess: () => setDraft("") },
              );
            }}
          />
        </div>
      </div>
      {rightPanelOpen && <ChatDetailsPanel modelId={convo.model} />}
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

/** Right-hand "in this chat" panel (static in Phase 1; later shows files, tools and tasks). */
function ChatDetailsPanel({ modelId }: { modelId: string }) {
  const toggle = useUi((s) => s.toggleRightPanel);
  const thinking = useUi((s) => s.thinking);
  const modelLabel = useModelLabel(modelId);
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
          </Row>
          <Row icon={<Plug className="size-4" />} label="Tools">
            None connected
          </Row>
          <Row icon={<FileText className="size-4" />} label="Files">
            None yet
          </Row>
        </dl>
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
