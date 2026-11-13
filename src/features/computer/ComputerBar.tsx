import { QueryClientProvider } from "@tanstack/react-query";
import { MonitorSmartphone, Square } from "lucide-react";
import { useState } from "react";
import { useThemeSync } from "@/app/useThemeSync";
import { createQueryClient } from "@/lib/api";
import { useComputerActivity, useStopComputer } from "./queries";

/**
 * Root of the control bar (`index.html?window=computer`): a small always-on-top window at
 * the top of the screen while a chat controls the computer. It never takes focus, so Stop
 * leaves the app being used in front.
 */
export function ComputerBarRoot() {
  const [client] = useState(createQueryClient);
  return (
    <QueryClientProvider client={client}>
      <Themed />
    </QueryClientProvider>
  );
}

function Themed() {
  useThemeSync();
  return <ComputerBar />;
}

/** "Oxide is controlling your computer · Typing “hello” · Stop (Ctrl+Alt+Esc)". */
export function ComputerBar() {
  const chats = useComputerActivity();
  const stop = useStopComputer();
  const working = chats.find((c) => c.state === "working");
  const waiting = !working && chats.some((c) => c.state === "waiting");
  const detail = working?.action ?? (waiting ? "Waiting for you in Oxide" : null);
  return (
    <div
      role="status"
      aria-label="Oxide is controlling your computer"
      className="flex h-screen w-screen select-none items-center gap-2.5 overflow-hidden border border-accent bg-raised px-3 text-sm text-fg"
    >
      <span className="relative flex size-2.5 shrink-0">
        <span
          aria-hidden
          className="absolute inline-flex size-full animate-ping rounded-full bg-accent opacity-60"
        />
        <span aria-hidden className="relative inline-flex size-2.5 rounded-full bg-accent" />
      </span>
      <MonitorSmartphone aria-hidden className="size-4 shrink-0 text-fg-muted" />
      <span className="shrink-0 font-medium">Oxide is controlling your computer</span>
      {detail && <span className="min-w-0 truncate text-fg-subtle">· {detail}</span>}
      {chats.length > 1 && <span className="shrink-0 text-fg-subtle">({chats.length} chats)</span>}
      <button
        type="button"
        disabled={stop.isPending}
        onClick={() => stop.mutate()}
        title="Stop (Ctrl+Alt+Esc)"
        className="ml-auto inline-flex h-7 shrink-0 items-center gap-1.5 rounded-lg bg-danger px-2.5 text-sm font-medium text-app hover:opacity-90 disabled:opacity-60"
      >
        <Square aria-hidden className="size-3 fill-current" />
        Stop
      </button>
    </div>
  );
}
