/**
 * Compact tool history (Phase 24): a reply's consecutive tool calls, and the thinking between
 * them, fold into one group that shows as a single summary line. Prose ends a group. Calls
 * whose output must stay visible (artifacts, generated media, computer use, the browser's
 * screenshots, Python's charts) stay outside the fold, in place, and split the groups around
 * them.
 */
import type { Compaction } from "@/bindings";
import type { ToolCallView } from "@/features/chat/streams";
import { isPlayable } from "@/features/generation/media";
import type { MessageKey, T } from "@/i18n";
import { callHeadline, toolKind } from "./toolView";

/** One piece of a reply, saved or live, in order. */
export type TranscriptItem =
  | { kind: "text"; text: string }
  | { kind: "thinking"; text: string; seconds: number; liveSince?: number }
  | { kind: "tool"; call: ToolCallView }
  | { kind: "compaction"; compaction: Compaction };

/** A step inside a group. */
export type GroupStep = Extract<TranscriptItem, { kind: "thinking" | "tool" }>;

/** What the reply view draws, in order. */
export type TranscriptEntry =
  | Extract<TranscriptItem, { kind: "text" | "thinking" | "compaction" }>
  /** A call drawn in full, outside any group. */
  | { kind: "outside"; call: ToolCallView }
  /** Folded tool work; `key` is its first call's id (stable while a reply streams). */
  | { kind: "group"; key: string; steps: GroupStep[] };

/** The call is waiting on an approval card the user must see. */
export function isWaiting(call: ToolCallView): boolean {
  return call.status === "awaitingApproval" && call.approval !== null;
}

/** The call is still being written, checked or run. */
export function isBusy(call: ToolCallView): boolean {
  return (
    call.status === "streaming" ||
    call.status === "ready" ||
    call.status === "approved" ||
    call.status === "awaitingApproval" ||
    call.status === "running"
  );
}

/** Calls whose output is never folded away. */
export function staysOutside(call: ToolCallView): boolean {
  const kind = toolKind(call);
  const result = call.result ?? [];
  const done = call.result !== null && !call.isError;
  switch (kind) {
    case "artifact":
      return call.status !== "error";
    case "computer":
      return true;
    case "generation":
      return done && result.some((b) => b.type === "image" || isPlayable(b));
    case "python":
      return done && result.some((b) => b.type === "image" || b.type === "artifactRef");
    case "browser":
      return call.status === "running" || (done && result.some((b) => b.type === "image"));
    default:
      return false;
  }
}

/** Groups a reply's items (see the module comment). */
export function groupItems(items: TranscriptItem[]): TranscriptEntry[] {
  const out: TranscriptEntry[] = [];
  let steps: GroupStep[] | null = null;
  const close = () => {
    if (steps) {
      const first = steps.find((s) => s.kind === "tool");
      if (first?.kind === "tool") out.push({ kind: "group", key: first.call.id, steps });
      else out.push(...steps.filter((s) => s.kind === "thinking"));
      steps = null;
    }
  };
  items.forEach((item, i) => {
    if (item.kind === "tool") {
      if (staysOutside(item.call)) {
        close();
        out.push({ kind: "outside", call: item.call });
      } else {
        (steps ??= []).push(item);
      }
      return;
    }
    if (item.kind === "thinking") {
      if (steps) {
        steps.push(item);
        return;
      }
      // Thinking joins a group when tool work follows it.
      let j = i + 1;
      while (items[j]?.kind === "thinking") j++;
      const next = items[j];
      if (next?.kind === "tool" && !staysOutside(next.call)) steps = [item];
      else out.push(item);
      return;
    }
    close();
    out.push(item);
  });
  close();
  return out;
}

/** The kind of thing a call did, for the summary. */
type SumKind =
  | "listDir"
  | "readFile"
  | "searchFiles"
  | "writeFile"
  | "editFile"
  | "moveFile"
  | "deleteFile"
  | "command"
  | "python"
  | "webSearch"
  | "webFetch"
  | "project"
  | "tasks"
  | "loadSkill"
  | "saveSkill"
  | "sandboxRun"
  | "sandboxFiles"
  | "browser"
  | "chrome"
  | "computer"
  | "image"
  | "video"
  | "music"
  | "artifact"
  | "connector";

const FILE_KINDS: Record<string, SumKind> = {
  list_dir: "listDir",
  read_file: "readFile",
  search_files: "searchFiles",
  write_file: "writeFile",
  edit_file: "editFile",
  move_file: "moveFile",
  delete_file: "deleteFile",
};

function sumKind(call: ToolCallView): SumKind {
  switch (toolKind(call)) {
    case "file":
      return FILE_KINDS[call.tool] ?? "readFile";
    case "shell":
      return "command";
    case "python":
      return "python";
    case "search":
      return "webSearch";
    case "fetch":
      return "webFetch";
    case "project":
      return "project";
    case "tasks":
      return "tasks";
    case "skill":
      return call.tool === "save_skill" ? "saveSkill" : "loadSkill";
    case "sandbox":
      return call.tool === "sandbox_exec" ? "sandboxRun" : "sandboxFiles";
    case "browser":
      return "browser";
    case "chrome":
      return "chrome";
    case "computer":
      return "computer";
    case "generation":
      return call.tool === "generate_video"
        ? "video"
        : call.tool === "generate_music"
          ? "music"
          : "image";
    case "artifact":
      return "artifact";
    case "connector":
      return "connector";
  }
}

/** One part of a summary: what was done and how often. */
export interface SummaryPart {
  kind: SumKind;
  /** The connector's name (connectors only). */
  name: string | null;
  count: number;
}

export interface GroupSummary {
  parts: SummaryPart[];
  failed: number;
}

/** What a group's calls did, by kind in order of first use, with failures counted. */
export function summarizeSteps(steps: GroupStep[]): GroupSummary {
  const parts = new Map<string, SummaryPart>();
  let failed = 0;
  for (const s of steps) {
    if (s.kind !== "tool") continue;
    const kind = sumKind(s.call);
    const name = kind === "connector" ? (s.call.server ?? s.call.title ?? s.call.tool) : null;
    const key = `${kind}:${name ?? ""}`;
    const part = parts.get(key) ?? { kind, name, count: 0 };
    part.count += 1;
    parts.set(key, part);
    if (s.call.status === "error") failed += 1;
  }
  return { parts: [...parts.values()], failed };
}

function capitalize(text: string): string {
  return text.charAt(0).toLocaleUpperCase() + text.slice(1);
}

/** “Listed a folder 4 times, ran 3 commands, wrote a file” in the interface language. */
export function summaryText(summary: GroupSummary, t: T): string {
  const words = summary.parts.map((p) => {
    const key = `toolsum.${p.kind}.${p.count === 1 ? "one" : "many"}` as MessageKey;
    return t(key, { n: p.count, name: p.name ?? "" });
  });
  return capitalize(words.join(", "));
}

/** “1 failed”, or `null` when nothing failed. */
export function failedText(summary: GroupSummary, t: T): string | null {
  return summary.failed > 0 ? t("toolsum.failed", { n: summary.failed }) : null;
}

/** A thinking step's short form: its first sentence, cut to 120 characters. */
export function firstSentence(text: string): string {
  const flat = text.replace(/\s+/g, " ").trim();
  const m = /^(.+?[.!?])(\s|$)/.exec(flat);
  const sentence = m?.[1] ?? flat;
  const chars = [...sentence];
  return chars.length > 120 ? `${chars.slice(0, 120).join("")}…` : sentence;
}

/** The newest finished sentence of a thought still being written (≤ 90 characters), or "". */
export function latestSentence(text: string): string {
  const flat = text.replace(/\s+/g, " ").trim();
  const sentences = flat.match(/[^.!?]+[.!?]+(?=\s|$)/g) ?? [];
  const last = sentences[sentences.length - 1]?.trim() ?? "";
  const chars = [...last];
  return chars.length > 90 ? `${chars.slice(0, 90).join("")}…` : last;
}

/** What a running group's status line says, and which spinner it shows. */
export interface LiveStatus {
  label: string;
  thinking: boolean;
}

/**
 * The running step (“Running mkdir PDFs”, “Safety check”, “Waiting for your approval”), else
 * the thought being written (its newest sentence), else what the group did so far. When the
 * group is `open` its rows show the running step, so the line says what is done.
 */
export function liveStatus(steps: GroupStep[], t: T, open = false): LiveStatus {
  const last = steps[steps.length - 1];
  const thinkingNow = last?.kind === "thinking" && last.liveSince !== undefined;
  const busy = steps.flatMap((s) => (s.kind === "tool" && isBusy(s.call) ? [s.call] : [])).at(-1);
  if (open) {
    const done = summarizeSteps(steps.filter((s) => s.kind === "tool" && !isBusy(s.call)));
    return {
      label: done.parts.length > 0 ? summaryText(done, t) : t("toolsum.working"),
      thinking: thinkingNow && !busy,
    };
  }
  if (busy) {
    if (busy.status === "awaitingApproval")
      return { label: t("toolsteps.waiting"), thinking: false };
    if (busy.checking && (busy.status === "ready" || busy.status === "streaming")) {
      return { label: t("toolsteps.safetyCheck"), thinking: false };
    }
    const { action, detail } = callHeadline(busy);
    return { label: detail ? `${action} ${detail}` : action, thinking: false };
  }
  if (last?.kind === "thinking" && thinkingNow) {
    return { label: latestSentence(last.text) || t("toolsteps.thinking"), thinking: true };
  }
  const done = summarizeSteps(steps);
  return {
    label: done.parts.length > 0 ? summaryText(done, t) : t("toolsum.working"),
    thinking: false,
  };
}
