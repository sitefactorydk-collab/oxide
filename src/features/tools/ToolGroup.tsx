import { ChevronRight, Loader2 } from "lucide-react";
import { useEffect, useState } from "react";
import type { ToolCallView } from "@/features/chat/streams";
import { AutoMarkIcon } from "@/features/permissions/AutoMarkBadge";
import { useToolSteps } from "@/features/settings/queries";
import { useT } from "@/i18n";
import { cn } from "@/lib/cn";
import { ApprovalCard, CallDetails, KindIcon, SavedSkillLink, StatusBadge } from "./ToolCallBlock";
import {
  failedText,
  firstSentence,
  isBusy,
  isWaiting,
  liveStatus,
  summarizeSteps,
  summaryText,
  type GroupStep,
} from "./toolGroups";
import { useToolReveal } from "./reveal";
import { Clock, Spinner, StatusLabel } from "./StatusLine";
import { useSteadyLabel } from "./useSteadyLabel";
import { useAutoMark } from "./useAutoMark";
import { callHeadline, savedSkillFolder, toolKind } from "./toolView";

/**
 * A reply's folded tool work (Phase 24): one muted summary line that opens to a box of
 * compact rows. While its reply runs (`live`) the line is a status line: a dot spinner, the
 * running step, the thought being written or the work so far (fading from one to the next),
 * and the reply's time. A waiting approval keeps the group open; Settings → Appearance → Show
 * tool steps: expanded starts every group open.
 */
export function ToolGroup({
  steps,
  live = false,
  startedAt,
}: {
  steps: GroupStep[];
  /** The reply is still running and this is its newest group. */
  live?: boolean;
  /** When the reply started (the status line's timer). */
  startedAt?: number;
}) {
  const t = useT();
  const setting = useToolSteps();
  const calls = steps.flatMap((s) => (s.kind === "tool" ? [s.call] : []));
  const waiting = calls.some(isWaiting);
  const [userOpen, setUserOpen] = useState<boolean | null>(null);
  const revealed = useToolReveal((s) =>
    s.id !== null && calls.some((c) => c.id === s.id) ? s.nonce : 0,
  );
  useEffect(() => {
    if (revealed) setUserOpen(true);
  }, [revealed]);
  const open = waiting || (userOpen ?? setting === "expanded");
  const busy =
    calls.some(isBusy) || steps.some((s) => s.kind === "thinking" && s.liveSince !== undefined);
  // The line names finished work; running calls show in the rows.
  const finished = summarizeSteps(steps.filter((s) => s.kind === "tool" && !isBusy(s.call)));
  const all = summarizeSteps(steps);
  const text = finished.parts.length > 0 ? summaryText(finished, t) : t("toolsum.working");
  const failed = failedText(all, t);
  const status = live ? liveStatus(steps, t, open) : null;
  const shownLabel = useSteadyLabel(status?.label ?? text, status?.thought ?? false);

  return (
    <div role="group" aria-label={t("toolsteps.label")} className="flex flex-col gap-1.5">
      {status ? (
        <button
          type="button"
          aria-expanded={open}
          aria-label={shownLabel}
          onClick={() => {
            if (!waiting) setUserOpen(!open);
          }}
          className="inline-flex w-fit max-w-full items-center gap-2.5 rounded-md py-1 text-left text-md text-fg-muted hover:text-fg"
        >
          <Spinner thinking={status.thinking} />
          <StatusLabel label={shownLabel} />
          {startedAt !== undefined && <Clock since={startedAt} />}
          <ChevronRight
            aria-hidden
            className={cn("size-4 shrink-0 transition-transform", open && "rotate-90")}
          />
        </button>
      ) : (
        <button
          type="button"
          aria-expanded={open}
          aria-label={`${text}${failed ? ` · ${failed}` : ""}`}
          onClick={() => {
            if (!waiting) setUserOpen(!open);
          }}
          className="inline-flex w-fit max-w-full items-center gap-2 rounded-md py-1 text-left text-md text-fg-muted hover:text-fg"
        >
          {busy && <Loader2 aria-hidden className="size-3.5 shrink-0 animate-spin" />}
          <span className="min-w-0 truncate">{text}</span>
          {failed && <span className="shrink-0 text-danger">· {failed}</span>}
          <ChevronRight
            aria-hidden
            className={cn("size-4 shrink-0 transition-transform", open && "rotate-90")}
          />
        </button>
      )}
      {open && (
        <ul
          aria-label={text}
          className="divide-y divide-line overflow-hidden rounded-xl border border-line bg-raised"
        >
          {steps.map((s, i) =>
            s.kind === "tool" ? (
              <CallRow key={s.call.id} call={s.call} />
            ) : (
              <ThinkingRow key={`t${i}`} step={s} />
            ),
          )}
        </ul>
      )}
    </div>
  );
}

/** “Ran mkdir …” with its mark and status; opens to the request and result. */
function CallRow({ call }: { call: ToolCallView }) {
  const [open, setOpen] = useState(false);
  const { action, detail } = callHeadline(call);
  const mark = useAutoMark(call);
  const waiting = isWaiting(call);
  return (
    <li data-tool-call={call.id} className={cn("flex flex-col", waiting && "bg-app")}>
      <button
        type="button"
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
        className="flex w-full min-w-0 items-center gap-2 px-3 py-1.5 text-left text-sm text-fg-muted hover:text-fg"
      >
        <KindIcon call={call} className="size-3.5 text-fg-subtle" />
        <span className="shrink-0 text-fg">{action}</span>
        {detail && <span className="min-w-0 truncate text-fg-subtle">{detail}</span>}
        {call.server && toolKind(call) === "connector" && (
          <span className="shrink-0 rounded bg-sunken px-1.5 py-0.5 font-mono text-[11px] text-fg-subtle">
            {call.server}
          </span>
        )}
        <span className="ml-auto" />
        {mark && <AutoMarkIcon mark={mark} />}
        <StatusBadge call={call} />
        <ChevronRight
          aria-hidden
          className={cn("size-3.5 shrink-0 transition-transform", open && "rotate-90")}
        />
      </button>
      {open && (
        <div className="px-3 pb-3">
          <CallDetails call={call} />
        </div>
      )}
      {call.server === null &&
        call.tool === "save_skill" &&
        call.status === "done" &&
        !call.isError && <SavedSkillLink folder={savedSkillFolder(call.inputJson)} />}
      {waiting && call.approval && <ApprovalCard request={call.approval} />}
    </li>
  );
}

/** A thinking step: its first sentence, muted; click for all of it. */
function ThinkingRow({ step }: { step: Extract<GroupStep, { kind: "thinking" }> }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const live = step.liveSince !== undefined;
  const short =
    firstSentence(step.text) ||
    (live ? t("toolsteps.thinking") : t("toolsteps.thought", { n: step.seconds }));
  return (
    <li className="flex flex-col">
      <button
        type="button"
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
        className="flex w-full min-w-0 items-center gap-2 px-3 py-1.5 text-left text-sm text-fg-subtle hover:text-fg-muted"
      >
        <span aria-hidden className="inline-flex w-3.5 shrink-0 items-center gap-[2px] text-accent">
          <span className={cn("size-[4px] rounded-full bg-current opacity-80", live && "ox-dot")} />
          <span className={cn("size-[4px] rounded-full bg-current opacity-50", live && "ox-dot")} />
        </span>
        <span className="min-w-0 truncate">{short}</span>
        <ChevronRight
          aria-hidden
          className={cn("ml-auto size-3.5 shrink-0 transition-transform", open && "rotate-90")}
        />
      </button>
      {open && (
        <div className="select-text whitespace-pre-wrap px-3 pb-3 pl-8 text-sm leading-6 text-fg-muted">
          {step.text}
        </div>
      )}
    </li>
  );
}
