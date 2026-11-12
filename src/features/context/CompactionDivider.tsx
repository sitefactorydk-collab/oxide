import { ChevronRight } from "lucide-react";
import { useState } from "react";
import type { Compaction } from "@/bindings";
import { Markdown } from "@/components/markdown/Markdown";
import { cn } from "@/lib/cn";
import { tokenCount } from "./queries";

const WHY: Record<Compaction["trigger"], string> = {
  auto: "to fit the model’s context window",
  manual: "on request",
  overflow: "after the model said its context was full",
};

/**
 * "Conversation compacted" in the transcript: from here on, replies start from a summary of
 * what came before. The summary can be opened.
 */
export function CompactionDivider({ compaction }: { compaction: Compaction }) {
  const [open, setOpen] = useState(false);
  return (
    <div role="note" aria-label="Conversation compacted" className="flex flex-col gap-2 py-1">
      <div className="flex items-center gap-3 text-sm text-fg-subtle">
        <span aria-hidden className="h-px flex-1 bg-line" />
        <button
          type="button"
          aria-expanded={open}
          onClick={() => setOpen((o) => !o)}
          className="inline-flex items-center gap-1.5 rounded-md px-1.5 py-0.5 hover:bg-hover hover:text-fg"
        >
          Conversation compacted
          <ChevronRight className={cn("size-3.5 transition-transform", open && "rotate-90")} />
        </button>
        <span aria-hidden className="h-px flex-1 bg-line" />
      </div>
      <p className="text-center text-xs text-fg-subtle">
        Summarized {WHY[compaction.trigger]}
        {compaction.tokensBefore > 0 &&
          ` · ${tokenCount(compaction.tokensBefore)} → ${tokenCount(compaction.tokensAfter)} tokens`}
      </p>
      {open && (
        <div className="rounded-xl border border-line bg-sunken px-4 py-3">
          <Markdown text={compaction.summary} className="ox-prose select-text text-base" />
        </div>
      )}
    </div>
  );
}
