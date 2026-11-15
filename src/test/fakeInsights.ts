import type { CommandError, Result, UsageReport, commands as RealCommands } from "@/bindings";

/**
 * Settings → Usage, About's start-up numbers and dictation in the fake backend (Phase 18).
 * `setFakeUsage()` sets the report; `setFakeTranscript()` what a recording turns into (or
 * an error); recordings are kept in `fakeInsights().recordings`.
 */
interface FakeInsights {
  usage: UsageReport;
  usageCalls: Array<{ days: number; offsetMinutes: number }>;
  transcript: string | { error: string };
  recordings: Array<{ audio: string; mime: string }>;
  ready: number;
}

let f: FakeInsights;

const totals = (input: number, output: number, replies: number) => ({
  replies,
  inputTokens: input,
  outputTokens: output,
  cacheReadTokens: 0,
  cacheWriteTokens: 0,
});

export function sampleUsage(): UsageReport {
  return {
    since: 0,
    totals: totals(152_000, 23_400, 42),
    chats: 9,
    unreported: 3,
    byProvider: [
      { providerId: "anthropic", totals: totals(120_000, 20_000, 30) },
      { providerId: "lmstudio", totals: totals(32_000, 3_400, 12) },
    ],
    byModel: [
      { providerId: "anthropic", model: "claude-sonnet-4-5", totals: totals(120_000, 20_000, 30) },
      { providerId: "lmstudio", model: "qwen/qwen3-14b", totals: totals(32_000, 3_400, 12) },
    ],
    days: Array.from({ length: 7 }, (_, i) => ({
      day: `2026-11-${String(9 + i).padStart(2, "0")}`,
      totals: totals(i * 1000, i * 100, i),
    })),
  };
}

export function resetFakeInsights(): void {
  f = {
    usage: sampleUsage(),
    usageCalls: [],
    transcript: "hello from the microphone",
    recordings: [],
    ready: 0,
  };
}
resetFakeInsights();

export function setFakeUsage(report: UsageReport): void {
  f.usage = report;
}

export function setFakeTranscript(t: string | { error: string }): void {
  f.transcript = t;
}

export function fakeInsights(): Readonly<FakeInsights> {
  return f;
}

const ok = <T>(data: T): Promise<Result<T, CommandError>> =>
  Promise.resolve({ status: "ok", data });

export const fakeInsightCommands: Pick<
  typeof RealCommands,
  "usageReport" | "appReady" | "appMetrics" | "transcribeAudio"
> = {
  usageReport: (days, offsetMinutes) => {
    f.usageCalls.push({ days, offsetMinutes });
    return ok(f.usage);
  },
  appReady: () => {
    f.ready += 1;
    return Promise.resolve();
  },
  appMetrics: () => Promise.resolve({ startupMs: 812, memoryBytes: 96 * 1024 * 1024 }),
  transcribeAudio: (audio, mime) => {
    f.recordings.push({ audio, mime });
    const t = f.transcript;
    return typeof t === "string"
      ? ok(t)
      : Promise.resolve({ status: "error", error: { kind: "invalid", message: t.error } });
  },
};
