/**
 * Static model list for the composer's picker. Phase 3 replaces it with per-provider
 * discovery and capability checks; ids are `<provider>/<model>` as stored on conversations.
 */
export interface ModelOption {
  id: string;
  label: string;
  provider: "Anthropic" | "OpenAI" | "LM Studio" | "Strata";
  note?: string;
}

export const MODEL_CATALOG: ModelOption[] = [
  { id: "anthropic/opus", label: "Opus", provider: "Anthropic", note: "Most capable" },
  { id: "anthropic/sonnet", label: "Sonnet", provider: "Anthropic", note: "Balanced" },
  { id: "anthropic/haiku", label: "Haiku", provider: "Anthropic", note: "Fastest" },
  { id: "openai/gpt", label: "GPT", provider: "OpenAI" },
  { id: "openai/gpt-mini", label: "GPT mini", provider: "OpenAI" },
  { id: "lmstudio/qwen3-14b", label: "qwen3-14b", provider: "LM Studio", note: "Local" },
  { id: "strata/default", label: "strata-default", provider: "Strata", note: "Local" },
];

export function modelLabel(id: string): string {
  return MODEL_CATALOG.find((m) => m.id === id)?.label ?? id.split("/").pop() ?? id;
}
