import type { ModelInfo, ProviderModels } from "@/bindings";

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
