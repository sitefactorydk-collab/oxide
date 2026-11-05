import type { GenerateOptions } from "@/bindings";
import { useUi } from "@/stores/ui";

/**
 * The composer's thinking and effort choices, for a send. The backend applies them per
 * model (levels a model doesn't have fall back to the nearest lower one).
 */
export function useGenerateOptions(): GenerateOptions {
  const thinking = useUi((s) => s.thinking);
  const effort = useUi((s) => s.effort);
  return { thinking, effort };
}
