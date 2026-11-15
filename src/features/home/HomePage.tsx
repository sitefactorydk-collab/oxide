import { BookOpen, Code, ListChecks, PenLine, ScrollText, type LucideIcon } from "lucide-react";
import { useEffect, useState } from "react";
import { useLocation, useNavigate } from "react-router";
import { OxideMark } from "@/components/OxideMark";
import { Composer } from "@/features/composer/Composer";
import { useStartConversation } from "@/features/chats/queries";
import type { NewChatState } from "@/features/desktop/keys";
import { useGenerateOptions } from "@/features/composer/options";
import { useNewChatModel } from "@/features/models/queries";
import { errorMessage } from "@/lib/api";
import { useT, type MessageKey } from "@/i18n";
import { useUi } from "@/stores/ui";

// Prompts stay English: they're the start of a message to a model, not interface text.
const CHIPS: Array<{ label: MessageKey; icon: LucideIcon; prompt: string }> = [
  { label: "home.chip.draft", icon: PenLine, prompt: "Help me draft " },
  { label: "home.chip.explain", icon: BookOpen, prompt: "Explain " },
  { label: "home.chip.code", icon: Code, prompt: "Write code that " },
  { label: "home.chip.plan", icon: ListChecks, prompt: "Make a plan for " },
  { label: "home.chip.summarize", icon: ScrollText, prompt: "Summarize " },
];

/** The greeting's key for a local hour (0–23). */
function greetingKey(hour: number): MessageKey {
  if (hour >= 5 && hour < 12) return "home.morning";
  if (hour >= 12 && hour < 18) return "home.afternoon";
  if (hour >= 18 && hour < 23) return "home.evening";
  return "home.late";
}

export function HomePage() {
  const userName = useUi((s) => s.userName);
  const [modelId, setModelId] = useNewChatModel();
  const options = useGenerateOptions();
  const start = useStartConversation();
  const navigate = useNavigate();
  const location = useLocation();
  const incoming = (location.state as NewChatState | null)?.draft;
  const [draft, setDraft] = useState(incoming ?? "");
  // A deep link (`oxide://new?q=…`) or the tray can fill the composer while this page is open.
  useEffect(() => {
    if (incoming !== undefined) setDraft(incoming);
  }, [incoming, location.key]);
  const [hour] = useState(() => new Date().getHours());
  const t = useT();

  const name = userName.trim();
  const greeting = `${t(greetingKey(hour))}${name ? `, ${name}` : ""}`;

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-y-auto">
      <div className="mx-auto flex w-full max-w-thread flex-1 flex-col justify-center px-6 pb-[12vh] pt-10">
        <h1 className="mb-8 flex items-center justify-center gap-3 text-center font-serif text-greeting font-normal leading-tight tracking-tight text-fg">
          <OxideMark className="size-10 shrink-0" />
          <span>{greeting}</span>
        </h1>
        <Composer
          variant="home"
          value={draft}
          onChange={setDraft}
          autoFocus
          modelId={modelId}
          onModelChange={setModelId}
          onSubmit={(text, attachments) => {
            start.mutate(
              {
                input: { text: text.trim(), model: modelId, projectId: null, attachments },
                options,
              },
              {
                onSuccess: (c) => {
                  setDraft("");
                  navigate(`/chat/${c.id}`);
                },
              },
            );
          }}
        />
        {start.error && (
          <p role="alert" className="mt-3 text-center text-danger">
            {t("home.startFailed", { error: errorMessage(start.error) })}
          </p>
        )}
        <div className="mt-4 flex flex-wrap justify-center gap-2">
          {CHIPS.map(({ label, icon: Icon, prompt }) => (
            <button
              key={label}
              type="button"
              onClick={() => setDraft(prompt)}
              className="inline-flex h-9 items-center gap-2 rounded-lg border border-line bg-app px-3 text-base text-fg-muted transition-colors hover:bg-hover hover:text-fg"
            >
              <Icon className="size-4" />
              {t(label)}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
