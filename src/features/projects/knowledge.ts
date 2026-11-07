import type { ProjectKnowledge } from "@/bindings";

/** "1.2k", "12k", "950". */
export function formatTokens(n: number): string {
  if (n < 1000) return String(n);
  const k = n / 1000;
  return `${k < 10 ? k.toFixed(1).replace(/\.0$/, "") : Math.round(k)}k`;
}

/** Share of the knowledge budget the files use, 0–100 (can't exceed 100 on the bar). */
export function budgetPercent(k: Pick<ProjectKnowledge, "totalTokens" | "budgetTokens">): number {
  if (k.budgetTokens <= 0) return 100;
  return Math.min(100, Math.round((k.totalTokens / k.budgetTokens) * 100));
}

/** The line under the knowledge bar. */
export function knowledgeSummary(k: ProjectKnowledge): string {
  switch (k.mode) {
    case "empty":
      return "No files yet. Add documents, code or notes for chats in this project to use.";
    case "full":
      return `${budgetPercent(k)}% of the knowledge budget. Chats get every file in full.`;
    case "search":
      return `More than the knowledge budget (${formatTokens(k.totalTokens)} of ${formatTokens(
        k.budgetTokens,
      )} tokens). Chats search the files instead of reading them all.`;
  }
}
