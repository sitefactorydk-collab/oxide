export const usageKeys = {
  report: (days: number) => ["usage", days] as const,
};

/** 1234 → "1.2K", 3_400_000 → "3.4M". */
export function compactNumber(n: number): string {
  if (n < 1000) return String(n);
  if (n < 1_000_000) return `${(n / 1000).toFixed(n < 10_000 ? 1 : 0)}K`;
  return `${(n / 1_000_000).toFixed(n < 10_000_000 ? 1 : 0)}M`;
}
