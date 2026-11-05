import { FileText, Lightbulb, Plug, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { Link, useParams } from "react-router";
import { IconButton } from "@/components/ui/IconButton";
import {
  useAddUserMessage,
  useConversation,
  useEditUserMessage,
  useMessages,
} from "@/features/chats/queries";
import { Composer } from "@/features/composer/Composer";
import { modelLabel } from "@/features/models/catalog";
import { errorMessage } from "@/lib/api";
import { useUi } from "@/stores/ui";
import { AssistantMessage, UserMessage } from "./Messages";
import { activeThread } from "./thread";

export function ChatPage() {
  const { id = "" } = useParams();
  // Keyed by id so the draft and scroll position reset when switching chats.
  return <ChatView key={id} id={id} />;
}

function ChatView({ id }: { id: string }) {
  const { conversation: convo, isPending: listPending } = useConversation(id);
  const messagesQuery = useMessages(convo ? id : "");
  const addMessage = useAddUserMessage();
  const editMessage = useEditUserMessage();
  const rightPanelOpen = useUi((s) => s.rightPanelOpen);
  const [draft, setDraft] = useState("");
  const scrollRef = useRef<HTMLDivElement>(null);

  const thread = useMemo(() => activeThread(messagesQuery.data ?? []), [messagesQuery.data]);
  const leaf = thread[thread.length - 1];

  useEffect(() => {
    const el = scrollRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [thread.length, id]);

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
  const sendError = addMessage.error ?? editMessage.error;

  return (
    <div className="flex min-h-0 flex-1">
      <div className="flex min-w-0 flex-1 flex-col">
        <div ref={scrollRef} className="min-h-0 flex-1 overflow-y-auto">
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
                  onEdit={(text) => editMessage.mutate({ messageId: m.id, text })}
                />
              ) : (
                <AssistantMessage key={m.id} message={m} isLast={i === lastAssistantIdx} />
              ),
            )}
            {leaf?.role === "user" && <NoReplyYet />}
            {sendError && (
              <p role="alert" className="text-danger">
                Couldn’t send: {errorMessage(sendError)}
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
            onSubmit={(text) => {
              const body = text.trim() || "(attachment)";
              addMessage.mutate(
                { conversationId: id, parentId: leaf?.id ?? null, text: body },
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

/** Shown under an unanswered message until model connections land in Phase 3. */
function NoReplyYet() {
  return (
    <p role="note" className="text-md text-fg-subtle">
      Saved. Oxide can’t reply yet: model connections arrive in Phase 3.
    </p>
  );
}

/** Right-hand "in this chat" panel (static in Phase 1; later shows files, tools and tasks). */
function ChatDetailsPanel({ modelId }: { modelId: string }) {
  const toggle = useUi((s) => s.toggleRightPanel);
  const thinking = useUi((s) => s.thinking);
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
            {modelLabel(modelId)}
            {thinking ? " · thinking" : ""}
          </Row>
          <Row icon={<Plug className="size-4" />} label="Tools">
            filesystem, web search
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
