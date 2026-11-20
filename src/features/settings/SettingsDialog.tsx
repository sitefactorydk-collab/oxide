import { PermissionsSection } from "@/features/tools/PermissionsSection";
import { WebSearchSection } from "@/features/tools/WebSearchSection";
import { MemorySection } from "@/features/memory/MemorySection";
import { DesktopSection } from "@/features/desktop/DesktopSection";
import { ExtensionsSection } from "@/features/extensions/ExtensionsSection";
import { SkillsSection } from "@/features/extensions/SkillsSection";
import { PythonSection } from "@/features/python/PythonSection";
import { BrowserSection } from "@/features/browser/BrowserSection";
import { ChromeSection } from "@/features/chrome/ChromeSection";
import { ComputerSection } from "@/features/computer/ComputerSection";
import { DataSection } from "@/features/data/DataSection";
import { UpdatesCard } from "@/features/data/UpdatesCard";
import { SandboxSection } from "@/features/sandbox/SandboxSection";
import { VoiceSection } from "@/features/dictation/VoiceSection";
import { GenerationSection } from "@/features/generation/GenerationSection";
import { UsageSection } from "@/features/usage/UsageSection";
import { useT, type MessageKey, type T } from "@/i18n";
import type { UiLanguage } from "@/bindings";
import { Monitor, Moon, Sun, X } from "lucide-react";
import { Dialog } from "radix-ui";
import { useEffect, useState, type ReactNode } from "react";
import { commands, type AppMetrics, type PingResponse } from "@/bindings";
import { OxideMark } from "@/components/OxideMark";
import { IconButton } from "@/components/ui/IconButton";
import { useNavigate } from "react-router";
import { useCreateDemoConversation } from "@/features/chats/queries";
import { useNewChatModel } from "@/features/models/queries";
import { errorMessage } from "@/lib/api";
import { cn } from "@/lib/cn";
import { type ChatFont, type ThemePref, useUi } from "@/stores/ui";
import { ConnectorsSection } from "@/features/mcp/ConnectorsSection";
import { DeveloperSection } from "@/features/mcp/DeveloperSection";
import { ModelsSection } from "@/features/models/ModelsSection";
import { ProvidersSection } from "@/features/providers/ProvidersSection";
import { useSetChatFont, useSetLanguage, useSetProfileName, useSetTheme } from "./queries";
import { findSection, SETTINGS_SECTIONS, type SettingsSection } from "./sections";

/** Settings: left nav + section pages. Sections become real in the phase they name. */
export function SettingsDialog({
  open,
  sectionId,
  onSelect,
  onClose,
}: {
  open: boolean;
  sectionId: string | undefined;
  onSelect: (id: string) => void;
  onClose: () => void;
}) {
  const section = findSection(sectionId);
  const t = useT();
  return (
    <Dialog.Root open={open} onOpenChange={(o) => !o && onClose()}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-40 bg-overlay" />
        <Dialog.Content
          aria-describedby={undefined}
          className="ox-pop fixed left-1/2 top-1/2 z-50 flex h-[min(680px,calc(100vh-64px))] w-[min(960px,calc(100vw-48px))] -translate-x-1/2 -translate-y-1/2 overflow-hidden rounded-2xl border border-line bg-app shadow-lg outline-none"
        >
          <nav
            aria-label={t("settings.sections")}
            className="flex w-56 shrink-0 flex-col gap-px overflow-y-auto border-r border-line bg-sidebar p-2"
          >
            <Dialog.Title className="px-2.5 pb-3 pt-2 font-serif text-xl font-normal text-fg">
              {t("settings.title")}
            </Dialog.Title>
            {SETTINGS_SECTIONS.map((s) => {
              const Icon = s.icon;
              const active = s.id === section.id;
              return (
                <button
                  key={s.id}
                  type="button"
                  aria-current={active ? "page" : undefined}
                  onClick={() => onSelect(s.id)}
                  className={cn(
                    "flex h-9 items-center gap-2.5 rounded-lg px-2.5 text-left text-md transition-colors",
                    active ? "bg-active text-fg" : "text-fg-muted hover:bg-hover hover:text-fg",
                  )}
                >
                  <Icon className="size-4 shrink-0" />
                  {sectionLabel(t, s)}
                </button>
              );
            })}
          </nav>
          <div className="relative flex min-w-0 flex-1 flex-col">
            <div className="absolute right-3 top-3">
              <Dialog.Close asChild>
                <IconButton tooltip={false} label={t("settings.close")}>
                  <X className="size-[18px]" />
                </IconButton>
              </Dialog.Close>
            </div>
            <div className="min-h-0 flex-1 overflow-y-auto px-10 py-8">
              <h2 className="mb-1 font-serif text-2xl font-normal text-fg">
                {sectionLabel(t, section)}
              </h2>
              <p className="mb-8 text-md text-fg-muted">{section.blurb}</p>
              <SectionBody section={section} />
            </div>
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

/** A section's name in the interface language (its English label when not translated). */
function sectionLabel(t: T, s: SettingsSection): string {
  const key = `section.${s.id}` as MessageKey;
  const text = t(key);
  return text === key ? s.label : text;
}

function SectionBody({ section }: { section: SettingsSection }) {
  switch (section.id) {
    case "voice":
      return <VoiceSection />;
    case "generation":
      return <GenerationSection />;
    case "usage":
      return <UsageSection />;
    case "general":
      return <GeneralSection />;
    case "appearance":
      return <AppearanceSection />;
    case "providers":
      return <ProvidersSection />;
    case "models":
      return <ModelsSection />;
    case "connectors":
      return <ConnectorsSection />;
    case "developer":
      return <DeveloperSection />;
    case "permissions":
      return <PermissionsSection />;
    case "web-search":
      return <WebSearchSection />;
    case "memory":
      return <MemorySection />;
    case "desktop":
      return <DesktopSection />;
    case "extensions":
      return <ExtensionsSection />;
    case "skills":
      return <SkillsSection />;
    case "advanced":
      return <PythonSection />;
    case "sandbox":
      return <SandboxSection />;
    case "browser":
      return <BrowserSection />;
    case "chrome":
      return <ChromeSection />;
    case "computer":
      return <ComputerSection />;
    case "data":
      return <DataSection />;
    case "about":
      return <AboutSection />;
    default:
      return (
        <div className="rounded-xl border border-dashed border-line-strong p-6 text-center text-fg-muted">
          This page arrives in Phase {section.phase}.
        </div>
      );
  }
}

function Field({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return (
    <div className="mb-8">
      <div className="mb-2 text-md font-medium text-fg">{label}</div>
      {children}
      {hint && <p className="mt-2 text-sm text-fg-subtle">{hint}</p>}
    </div>
  );
}

function GeneralSection() {
  const userName = useUi((s) => s.userName);
  const setUserName = useSetProfileName();
  return (
    <Field label="What should Oxide call you?" hint="Used in the greeting on the new chat screen.">
      <input
        aria-label="Your name"
        value={userName}
        onChange={(e) => setUserName(e.target.value)}
        placeholder="Your name"
        className="h-10 w-80 max-w-full rounded-lg border border-line bg-raised px-3 text-md text-fg outline-none focus:border-accent"
      />
    </Field>
  );
}

function Choice({
  selected,
  onClick,
  children,
  label,
}: {
  selected: boolean;
  onClick: () => void;
  children: ReactNode;
  label: string;
}) {
  return (
    <button
      type="button"
      role="radio"
      aria-checked={selected}
      aria-label={label}
      onClick={onClick}
      className={cn(
        "flex w-36 flex-col items-stretch gap-2 rounded-xl border p-2 text-left text-base transition-colors",
        selected ? "border-accent ring-2 ring-accent-soft" : "border-line hover:border-line-strong",
      )}
    >
      {children}
    </button>
  );
}

function ThemePreview({ mode }: { mode: "light" | "dark" | "split" }) {
  // Miniature window drawn with the real tokens of each theme (scoped via data-theme).
  const pane = (theme: "light" | "dark") => (
    <div data-theme={theme} className="flex h-full flex-1 bg-app">
      <div className="h-full w-1/3 border-r border-line bg-sidebar" />
      <div className="flex flex-1 flex-col justify-end gap-1 p-1.5">
        <div className="ml-auto h-1.5 w-2/3 rounded-full bg-bubble" />
        <div className="h-1.5 w-full rounded-full bg-line-strong" />
        <div className="h-3 w-full rounded-md border border-line bg-raised" />
      </div>
    </div>
  );
  return (
    <div className="flex h-20 overflow-hidden rounded-lg border border-line">
      {mode === "split" ? (
        <>
          {pane("light")}
          {pane("dark")}
        </>
      ) : (
        pane(mode)
      )}
    </div>
  );
}

function AppearanceSection() {
  const theme = useUi((s) => s.theme);
  const setTheme = useSetTheme();
  const chatFont = useUi((s) => s.chatFont);
  const setChatFont = useSetChatFont();
  const themes: Array<{
    id: ThemePref;
    label: string;
    icon: ReactNode;
    preview: "light" | "dark" | "split";
  }> = [
    { id: "light", label: "Light", icon: <Sun className="size-4" />, preview: "light" },
    { id: "dark", label: "Dark", icon: <Moon className="size-4" />, preview: "dark" },
    { id: "system", label: "Match system", icon: <Monitor className="size-4" />, preview: "split" },
  ];
  const fonts: Array<{ id: ChatFont; label: string; sample: string }> = [
    { id: "serif", label: "Serif", sample: "font-serif" },
    { id: "sans", label: "Sans", sample: "font-sans" },
    { id: "system", label: "System", sample: "[font-family:var(--ox-font-system)]" },
  ];
  return (
    <>
      <Field label="Theme">
        <div role="radiogroup" aria-label="Theme" className="flex flex-wrap gap-3">
          {themes.map((t) => (
            <Choice
              key={t.id}
              label={t.label}
              selected={theme === t.id}
              onClick={() => setTheme(t.id)}
            >
              <ThemePreview mode={t.preview} />
              <span className="flex items-center gap-2 px-1 text-fg">
                {t.icon}
                {t.label}
              </span>
            </Choice>
          ))}
        </div>
      </Field>
      <Field
        label="Chat font"
        hint="Used for replies. Your own messages and the interface always use Inter."
      >
        <div role="radiogroup" aria-label="Chat font" className="flex flex-wrap gap-3">
          {fonts.map((f) => (
            <Choice
              key={f.id}
              label={f.label}
              selected={chatFont === f.id}
              onClick={() => setChatFont(f.id)}
            >
              <span
                className={cn(
                  "flex h-14 items-center justify-center rounded-lg bg-sunken text-2xl text-fg",
                  f.sample,
                )}
              >
                Aa
              </span>
              <span className="px-1 text-fg">{f.label}</span>
            </Choice>
          ))}
        </div>
      </Field>
      <LanguageField />
    </>
  );
}

function LanguageField() {
  const t = useT();
  const language = useUi((s) => s.language);
  const setLanguage = useSetLanguage();
  const options: Array<{ id: UiLanguage; label: string }> = [
    { id: "system", label: t("settings.languageSystem") },
    { id: "en", label: "English" },
    { id: "da", label: "Dansk" },
  ];
  return (
    <Field label={t("settings.language")} hint={t("settings.languageHint")}>
      <div role="radiogroup" aria-label={t("settings.language")} className="flex flex-wrap gap-2">
        {options.map((o) => (
          <button
            key={o.id}
            type="button"
            role="radio"
            aria-checked={language === o.id}
            onClick={() => setLanguage(o.id)}
            className={cn(
              "h-9 rounded-lg border px-3 text-base transition-colors",
              language === o.id
                ? "border-accent text-fg ring-2 ring-accent-soft"
                : "border-line text-fg-muted hover:border-line-strong hover:text-fg",
            )}
          >
            {o.label}
          </button>
        ))}
      </div>
    </Field>
  );
}

function AboutSection() {
  const [info, setInfo] = useState<PingResponse | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [metrics, setMetrics] = useState<AppMetrics | null>(null);
  useEffect(() => {
    let alive = true;
    commands
      .ping("Oxide")
      .then((r) => alive && setInfo(r))
      .catch((e: unknown) => alive && setError(String(e)));
    commands
      .appMetrics()
      .then((m) => alive && setMetrics(m))
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, []);
  return (
    <div className="flex items-start gap-5">
      <OxideMark className="size-16" />
      <div className="text-md">
        <p className="font-serif text-xl text-fg">Oxide</p>
        <p className="text-fg-muted" data-testid="about-version">
          {info
            ? `Version ${info.appVersion} · core ${info.coreVersion}`
            : error
              ? "Backend not reachable (browser preview)"
              : "Checking backend…"}
        </p>
        {metrics && (metrics.startupMs !== null || metrics.memoryBytes !== null) && (
          <p className="text-sm text-fg-subtle" data-testid="about-metrics">
            {[
              metrics.startupMs !== null && `Started in ${(metrics.startupMs / 1000).toFixed(1)} s`,
              metrics.memoryBytes !== null &&
                `${Math.round(metrics.memoryBytes / (1024 * 1024))} MB in use (Oxide itself; the window’s web view counts separately)`,
            ]
              .filter(Boolean)
              .join(" · ")}
          </p>
        )}
        <p className="mt-4 max-w-md text-base text-fg-muted">
          A desktop AI assistant written in Rust on Tauri. Fonts: Inter, Source Serif 4 and
          JetBrains Mono (SIL Open Font License); KaTeX fonts for math (SIL OFL). Icons: Lucide
          (ISC). Syntax highlighting: Shiki (MIT).
        </p>
        <UpdatesCard />
        {import.meta.env.DEV && <DemoChatButton />}
      </div>
    </div>
  );
}

/** Development builds: a 50-message chat with code, math and images, for checking rendering. */
function DemoChatButton() {
  const [modelId] = useNewChatModel();
  const create = useCreateDemoConversation();
  const navigate = useNavigate();
  return (
    <div className="mt-6 rounded-xl border border-dashed border-line-strong p-4">
      <p className="text-base font-medium text-fg">Developer tools</p>
      <p className="mt-1 text-sm text-fg-muted">
        Creates a 50-message chat with code, math, tables, images and a PDF. Nothing is sent to a
        model.
      </p>
      <button
        type="button"
        disabled={create.isPending}
        onClick={() => create.mutate(modelId, { onSuccess: (c) => navigate(`/chat/${c.id}`) })}
        className="mt-3 h-8 rounded-lg border border-line bg-raised px-3 text-sm text-fg hover:bg-hover disabled:opacity-50"
      >
        {create.isPending ? "Creating…" : "Create demo chat"}
      </button>
      {create.error && <p className="mt-2 text-sm text-danger">{errorMessage(create.error)}</p>}
    </div>
  );
}
