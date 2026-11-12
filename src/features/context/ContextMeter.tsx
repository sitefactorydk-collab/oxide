import { Loader2, Minimize2 } from "lucide-react";
import { Popover as P } from "radix-ui";
import type { ContextUsage, WindowSource } from "@/bindings";
import { useStreams } from "@/features/chat/streams";
import { useOpenSettings } from "@/features/settings/useOpenSettings";
import { errorMessage } from "@/lib/api";
import { cn } from "@/lib/cn";
import { tokenCount, useChatContext, useCompactNow } from "./queries";

const SOURCE: Record<WindowSource, string> = {
  provider: "reported by the provider",
  setting: "set in Settings → Models",
  learned: "named by the model’s server when it was full",
  default: "not reported, so Oxide assumes it",
};

/**
 * The composer's context meter: how much of the model's context window the chat fills.
 * While a reply runs it follows the reply's requests; its popover explains the numbers and
 * has **Compact now**.
 */
export function ContextMeter({
  conversationId,
  leafId,
  modelId,
  streaming,
}: {
  conversationId: string;
  leafId: string | null;
  modelId: string;
  streaming: boolean;
}) {
  const live = useStreams((s) => s.live[conversationId]?.context ?? null);
  const compacting = useStreams((s) => s.live[conversationId]?.compacting ?? false);
  const { data: stored } = useChatContext(conversationId, leafId, modelId);
  const compact = useCompactNow();
  const openSettings = useOpenSettings();
  const usage: ContextUsage | undefined = (streaming ? live : null) ?? stored;
  if (!usage) return null;

  const share = usage.window > 0 ? Math.min(1, usage.tokens / usage.window) : 0;
  const percent = Math.round(share * 100);
  const near = usage.threshold > 0 && usage.tokens >= usage.threshold * 0.9;
  const busy = compacting || compact.isPending;
  const label = busy
    ? "Compacting the conversation"
    : `Context: ${percent}% of ${tokenCount(usage.window)} tokens used`;

  return (
    <P.Root>
      <P.Trigger asChild>
        <button
          type="button"
          aria-label={label}
          title={label}
          className="inline-flex h-8 items-center gap-1.5 rounded-lg px-1.5 text-xs text-fg-subtle hover:bg-hover hover:text-fg"
        >
          {busy ? (
            <Loader2 className="size-4 animate-spin text-accent" />
          ) : (
            <Ring share={share} warn={near} />
          )}
          <span className="tabular-nums">{percent}%</span>
        </button>
      </P.Trigger>
      <P.Portal>
        <P.Content
          side="top"
          align="end"
          sideOffset={8}
          className="ox-pop z-50 flex w-80 flex-col gap-3 rounded-xl border border-line bg-raised p-4 text-base text-fg shadow-lg outline-none"
        >
          <div className="flex flex-col gap-1.5">
            <div className="flex items-baseline justify-between">
              <span className="font-medium">Context window</span>
              <span className="tabular-nums text-sm text-fg-muted">
                {tokenCount(usage.tokens)} / {tokenCount(usage.window)}
              </span>
            </div>
            <div
              role="meter"
              aria-label="Context used"
              aria-valuemin={0}
              aria-valuemax={usage.window}
              aria-valuenow={usage.tokens}
              className="relative h-1.5 overflow-hidden rounded-full bg-sunken"
            >
              <div
                className={cn("h-full rounded-full", near ? "bg-danger" : "bg-accent")}
                style={{ width: `${percent}%` }}
              />
              {usage.threshold > 0 && (
                <div
                  aria-hidden
                  className="absolute inset-y-0 w-px bg-fg-muted"
                  style={{ left: `${Math.min(100, (usage.threshold / usage.window) * 100)}%` }}
                />
              )}
            </div>
          </div>
          <ul className="flex flex-col gap-1 text-sm text-fg-muted">
            <li>
              {usage.window.toLocaleString("en-US")} tokens: {SOURCE[usage.windowSource]}.
              {usage.windowSource === "default" && (
                <>
                  {" "}
                  <button
                    type="button"
                    onClick={() => openSettings("models")}
                    className="text-accent hover:underline"
                  >
                    Set the real window
                  </button>
                </>
              )}
            </li>
            <li>
              {usage.threshold > 0
                ? `Old tool results are cleared, then the chat is summarized, above ${tokenCount(usage.threshold)} tokens.`
                : "Automatic compaction is off (Settings → Models)."}
            </li>
            {usage.compacted && <li>Earlier messages are summarized to save room.</li>}
            {usage.clearedResults > 0 && (
              <li>
                {usage.clearedResults} old tool{" "}
                {usage.clearedResults === 1 ? "result was" : "results were"} cleared in the latest
                request.
              </li>
            )}
            <li className="text-fg-subtle">Estimates, corrected by what the model reports.</li>
          </ul>
          <button
            type="button"
            disabled={streaming || busy || !leafId}
            onClick={() => {
              if (leafId) compact.mutate({ conversationId, leafId });
            }}
            className="inline-flex h-8 items-center justify-center gap-2 rounded-lg border border-line px-3 text-sm text-fg hover:bg-hover disabled:opacity-50"
          >
            {busy ? <Loader2 className="size-4 animate-spin" /> : <Minimize2 className="size-4" />}
            {busy ? "Compacting…" : "Compact now"}
          </button>
          {compact.error && (
            <p role="alert" className="text-sm text-danger">
              {errorMessage(compact.error)}
            </p>
          )}
          <p className="text-xs text-fg-subtle">
            Compacting summarizes the conversation so far; later replies start from the summary. The
            transcript keeps every message.
          </p>
        </P.Content>
      </P.Portal>
    </P.Root>
  );
}

/** A small ring that fills with the share of the window used. */
function Ring({ share, warn }: { share: number; warn: boolean }) {
  const r = 6.5;
  const c = 2 * Math.PI * r;
  return (
    <svg viewBox="0 0 16 16" className="size-4 -rotate-90" aria-hidden>
      <circle cx="8" cy="8" r={r} fill="none" strokeWidth="2" className="stroke-line-strong" />
      <circle
        cx="8"
        cy="8"
        r={r}
        fill="none"
        strokeWidth="2"
        strokeLinecap="round"
        strokeDasharray={`${c * share} ${c}`}
        className={warn ? "stroke-danger" : "stroke-accent"}
      />
    </svg>
  );
}
