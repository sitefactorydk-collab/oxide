import type { Effort, ModelCapabilities, ModelInfo, ProviderModels } from "@/bindings";

/** All listed models, in endpoint order. */
export function allModels(lists: ProviderModels[]): ModelInfo[] {
  return lists.flatMap((l) => l.models);
}

export function findModel(lists: ProviderModels[], id: string): ModelInfo | undefined {
  for (const l of lists) {
    const m = l.models.find((x) => x.id === id);
    if (m) return m;
  }
  return undefined;
}

/** Whether a model can be picked (native tool calling is required). */
export function isUsable(m: ModelInfo): boolean {
  return m.capabilities.tools;
}

/** The model part of a stored id (`<providerId>/<model>`), for display when it isn't listed. */
export function modelNameFromId(id: string): string {
  const at = id.indexOf("/");
  return at >= 0 ? id.slice(at + 1) : id;
}

/**
 * The model new chats should use: `preferred` when it's listed and usable, otherwise a
 * sensible default (a balanced Anthropic model, then the first usable model anywhere).
 * `undefined` while nothing usable is listed.
 */
export function pickDefaultModel(lists: ProviderModels[], preferred: string): string | undefined {
  const usable = allModels(lists).filter(isUsable);
  if (usable.some((m) => m.id === preferred)) return preferred;
  const sonnet = usable.find((m) => m.providerId === "anthropic" && m.model.includes("sonnet"));
  return (sonnet ?? usable[0])?.id;
}

/** The endpoint listing a model belongs to. */
export function findListing(lists: ProviderModels[], id: string): ProviderModels | undefined {
  return lists.find((l) => l.models.some((m) => m.id === id));
}

export const EFFORT_ORDER: Effort[] = ["low", "medium", "high", "xhigh", "max"];

export const EFFORT_LABELS: Record<Effort, string> = {
  low: "Low",
  medium: "Medium",
  high: "High",
  xhigh: "Extra",
  max: "Max",
};

/**
 * The level a request will use: the requested one if the model accepts it, else the nearest
 * lower level it accepts (or its lowest). Mirrors `ModelCapabilities::effective_effort`.
 */
export function effectiveEffort(
  caps: Pick<ModelCapabilities, "effort">,
  requested: Effort | null,
): Effort | null {
  if (requested === null || caps.effort.length === 0) return null;
  if (caps.effort.includes(requested)) return requested;
  const rank = (e: Effort) => EFFORT_ORDER.indexOf(e);
  const lower = caps.effort.filter((e) => rank(e) <= rank(requested));
  const pool = lower.length ? lower : caps.effort;
  return pool.reduce((best, e) =>
    lower.length ? (rank(e) > rank(best) ? e : best) : rank(e) < rank(best) ? e : best,
  );
}

/** Whether the model thinks no matter what (adaptive thinking that can't be switched off). */
export function alwaysThinks(caps: ModelCapabilities): boolean {
  return caps.thinking === "adaptive" && !caps.thinkingOff;
}
