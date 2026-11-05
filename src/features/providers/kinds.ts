import type { ProviderKind } from "@/bindings";

/** Per-kind display data. Default URLs mirror `ProviderKind::default_base_url` in oxide-core. */
export const PROVIDER_KINDS: Record<
  ProviderKind,
  { label: string; defaultUrl: string; needsKey: boolean }
> = {
  anthropic: { label: "Anthropic", defaultUrl: "https://api.anthropic.com/v1", needsKey: true },
  openai: { label: "OpenAI", defaultUrl: "https://api.openai.com/v1", needsKey: true },
  lmstudio: { label: "LM Studio", defaultUrl: "http://127.0.0.1:1234/v1", needsKey: false },
  strata: { label: "Strata", defaultUrl: "http://127.0.0.1:47831/v1", needsKey: false },
};

export const KIND_ORDER: ProviderKind[] = ["anthropic", "openai", "lmstudio", "strata"];
