import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { commands, type UsageDay, type UsageTotals } from "@/bindings";
import { useProviders } from "@/features/providers/queries";
import { errorMessage, unwrap } from "@/lib/api";
import { cn } from "@/lib/cn";
import { compactNumber, usageKeys } from "./format";

const RANGES: Array<{ days: number; label: string }> = [
  { days: 7, label: "7 days" },
  { days: 30, label: "30 days" },
  { days: 90, label: "90 days" },
  { days: 0, label: "All time" },
];

const tokens = (t: UsageTotals) => t.inputTokens + t.outputTokens;

/** Settings → Usage: tokens per provider, model and day, from the saved replies. */
export function UsageSection() {
  const [days, setDays] = useState(30);
  const { data: providers } = useProviders();
  const report = useQuery({
    queryKey: usageKeys.report(days),
    queryFn: () => unwrap(commands.usageReport(days, -new Date().getTimezoneOffset())),
  });
  const providerName = (id: string) =>
    providers.find((p) => p.id === id)?.name ?? (id || "Unknown endpoint");
  const r = report.data;

  return (
    <div className="flex max-w-3xl flex-col gap-6">
      <div role="radiogroup" aria-label="Range" className="flex gap-1">
        {RANGES.map((x) => (
          <button
            key={x.days}
            type="button"
            role="radio"
            aria-checked={days === x.days}
            onClick={() => setDays(x.days)}
            className={cn(
              "h-8 rounded-lg px-3 text-sm transition-colors",
              days === x.days ? "bg-active text-fg" : "text-fg-muted hover:bg-hover hover:text-fg",
            )}
          >
            {x.label}
          </button>
        ))}
      </div>
      {report.error && <p className="text-danger">{errorMessage(report.error)}</p>}
      {r && (
        <>
          <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
            <Stat label="Tokens" value={compactNumber(tokens(r.totals))} />
            <Stat label="Input" value={compactNumber(r.totals.inputTokens)} />
            <Stat label="Output" value={compactNumber(r.totals.outputTokens)} />
            <Stat label="Replies" value={`${r.totals.replies}`} hint={`in ${r.chats} chats`} />
          </div>
          {r.days.length > 0 && <DayChart days={r.days} />}
          <Table
            title="By provider"
            rows={r.byProvider.map((p) => ({
              key: p.providerId,
              name: providerName(p.providerId),
              totals: p.totals,
            }))}
          />
          <Table
            title="By model"
            rows={r.byModel.map((m) => ({
              key: `${m.providerId}/${m.model}`,
              name: m.model || "Unknown model",
              sub: providerName(m.providerId),
              totals: m.totals,
            }))}
          />
          <p className="text-sm text-fg-subtle">
            Counted from saved replies, as each provider reported them
            {r.unreported > 0 &&
              ` (${r.unreported} ${r.unreported === 1 ? "reply" : "replies"} came without counts, typically from local servers)`}
            . Chat titles, summaries of long chats and memory updates aren’t included, and a deleted
            chat takes its usage with it. Cached input is part of Input.
          </p>
        </>
      )}
    </div>
  );
}

function Stat({ label, value, hint }: { label: string; value: string; hint?: string }) {
  return (
    <div className="rounded-xl border border-line bg-raised px-4 py-3">
      <div className="text-sm text-fg-muted">{label}</div>
      <div className="mt-0.5 text-2xl font-medium tabular-nums text-fg">{value}</div>
      {hint && <div className="text-xs text-fg-subtle">{hint}</div>}
    </div>
  );
}

/** One bar per day (input + output tokens), a tooltip with the numbers on hover. */
function DayChart({ days }: { days: UsageDay[] }) {
  const max = Math.max(1, ...days.map((d) => tokens(d.totals)));
  const [hover, setHover] = useState<UsageDay | null>(null);
  const shown = hover ?? null;
  return (
    <figure aria-label="Tokens per day" className="rounded-xl border border-line bg-raised p-4">
      <figcaption className="mb-3 flex items-baseline justify-between text-sm">
        <span className="font-medium text-fg">Tokens per day</span>
        <span className="tabular-nums text-fg-muted" data-testid="usage-hover">
          {shown
            ? `${shown.day}: ${compactNumber(tokens(shown.totals))} tokens · ${shown.totals.replies} replies`
            : `Peak ${compactNumber(max)}`}
        </span>
      </figcaption>
      <div
        className="flex h-32 items-end gap-[2px] border-b border-line"
        onMouseLeave={() => setHover(null)}
      >
        {days.map((d) => {
          const v = tokens(d.totals);
          return (
            <div
              key={d.day}
              role="img"
              aria-label={`${d.day}: ${v} tokens`}
              onMouseEnter={() => setHover(d)}
              className="flex h-full min-w-0 flex-1 items-end hover:bg-hover"
            >
              <div
                className={cn("w-full rounded-t-[4px] bg-accent", hover === d && "bg-accent-hover")}
                style={{ height: v > 0 ? `max(2px, ${(v / max) * 100}%)` : 0 }}
              />
            </div>
          );
        })}
      </div>
      <div className="mt-1 flex justify-between text-xs text-fg-subtle">
        <span>{days[0]?.day}</span>
        <span>{days[days.length - 1]?.day}</span>
      </div>
    </figure>
  );
}

function Table({
  title,
  rows,
}: {
  title: string;
  rows: Array<{ key: string; name: string; sub?: string; totals: UsageTotals }>;
}) {
  return (
    <section aria-label={title}>
      <h3 className="mb-2 text-md font-semibold text-fg">{title}</h3>
      {rows.length === 0 ? (
        <p className="text-sm text-fg-muted">Nothing in this range yet.</p>
      ) : (
        <table className="w-full text-sm">
          <thead>
            <tr className="border-b border-line text-left text-fg-muted">
              <th className="py-1.5 font-normal">Name</th>
              <th className="py-1.5 text-right font-normal">Replies</th>
              <th className="py-1.5 text-right font-normal">Input</th>
              <th className="py-1.5 text-right font-normal">Output</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((r) => (
              <tr key={r.key} className="border-b border-line last:border-0">
                <td className="py-1.5 text-fg">
                  {r.name}
                  {r.sub && <span className="ml-2 text-fg-subtle">{r.sub}</span>}
                </td>
                <td className="py-1.5 text-right tabular-nums text-fg-muted">{r.totals.replies}</td>
                <td className="py-1.5 text-right tabular-nums text-fg">
                  {compactNumber(r.totals.inputTokens)}
                </td>
                <td className="py-1.5 text-right tabular-nums text-fg">
                  {compactNumber(r.totals.outputTokens)}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </section>
  );
}
