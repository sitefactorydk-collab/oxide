import { FileText, Lightbulb, Plug, X } from "lucide-react";
import { useEffect, useRef, useState, type ReactNode } from "react";
import { Link, useParams } from "react-router";
import { IconButton } from "@/components/ui/IconButton";
import { Composer } from "@/features/composer/Composer";
import { modelLabel } from "@/features/mock/data";
import { messageText, useMock } from "@/features/mock/store";
import type { Message } from "@/features/mock/types";
import { useUi } from "@/stores/ui";
import { AssistantMessage, UserMessage, WorkingLine } from "./Messages";

const NO_MESSAGES: Message[] = [];

export function ChatPage() {
  const { id = "" } = useParams();
  // Keyed by id so the draft and scroll position reset when switching chats.
  return <ChatView key={id} id={id} />;
}

function ChatView({ id }: { id: string }) {
  const convo = useMock((s) => s.conversations.find((c) => c.id === id));
  const messages = useMock((s) => s.messages[id]) ?? NO_MESSAGES;
  const streaming = useMock((s) => s.streaming);
  const sendReply = useMock((s) => s.sendReply);
  const editMessage = useMock((s) => s.editMessage);
  const stop = useMock((s) => s.stop);
  const rightPanelOpen = useUi((s) => s.rightPanelOpen);
  const [draft, setDraft] = useState("");
  const scrollRef = useRef<HTMLDivElement>(null);

  const isStreaming = streaming?.conversationId === id;

  useEffect(() => {
    const el = scrollRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [messages.length, isStreaming, id]);

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
  messages.forEach((m, i) => {
    if (m.role === "assistant") lastAssistantIdx = i;
  });
  const retry = () => {
    const lastUser = [...messages].reverse().find((m) => m.role === "user");
    if (lastUser && !isStreaming) sendReply(id, messageText(lastUser));
  };

  return (
    <div className="flex min-h-0 flex-1">
      <div className="flex min-w-0 flex-1 flex-col">
        <div ref={scrollRef} className="min-h-0 flex-1 overflow-y-auto">
          <div className="mx-auto flex w-full max-w-thread flex-col gap-6 px-6 pb-10 pt-8">
            {messages.map((m, i) =>
              m.role === "user" ? (
                <UserMessage key={m.id} message={m} onEdit={(t) => editMessage(id, m.id, t)} />
              ) : (
                <AssistantMessage
                  key={m.id}
                  message={m}
                  isLast={i === lastAssistantIdx && !isStreaming}
                  onRetry={retry}
                />
              ),
            )}
            {isStreaming && streaming && <WorkingLine startedAt={streaming.startedAt} />}
          </div>
        </div>
        <div className="mx-auto w-full max-w-thread px-6 pb-3">
          <Composer
            variant="reply"
            value={draft}
            onChange={setDraft}
            streaming={isStreaming}
            onStop={stop}
            autoFocus
            onSubmit={(text) => {
              sendReply(id, text || "(attachment)");
              setDraft("");
            }}
          />
        </div>
      </div>
      {rightPanelOpen && <ChatDetailsPanel modelId={convo.modelId} />}
    </div>
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
