import { describe, expect, it } from "vitest";
import type { ModelInfo, ProviderModels } from "@/bindings";
import { effectiveEffort, findModel, modelNameFromId, pickDefaultModel } from "./models";

const model = (providerId: string, name: string, tools = true): ModelInfo => ({
  id: `${providerId}/${name}`,
  providerId,
  model: name,
  label: name,
  capabilities: {
    tools,
    thinking: "none",
    thinkingOff: false,
    images: false,
    pdfs: false,
    effort: [],
    defaultEffort: null,
  },
  contextWindow: null,
  maxOutputTokens: null,
});

const lists: ProviderModels[] = [
  {
    providerId: "anthropic",
    providerName: "Anthropic",
    providerKind: "anthropic",
    models: [model("anthropic", "claude-opus-5-5"), model("anthropic", "claude-sonnet-5-5")],
    error: null,
  },
  {
    providerId: "lmstudio",
    providerName: "LM Studio",
    providerKind: "lmstudio",
    models: [model("lmstudio", "qwen/qwen3-14b"), model("lmstudio", "no-tools", false)],
    error: null,
  },
];

describe("models", () => {
  it("keeps a usable preferred model", () => {
    expect(pickDefaultModel(lists, "lmstudio/qwen/qwen3-14b")).toBe("lmstudio/qwen/qwen3-14b");
  });

  it("falls back to a balanced Anthropic model, then any usable one", () => {
    expect(pickDefaultModel(lists, "anthropic/sonnet")).toBe("anthropic/claude-sonnet-5-5");
    expect(pickDefaultModel(lists, "lmstudio/no-tools")).toBe("anthropic/claude-sonnet-5-5");
    expect(pickDefaultModel(lists.slice(1), "x/y")).toBe("lmstudio/qwen/qwen3-14b");
    expect(pickDefaultModel([], "x/y")).toBeUndefined();
  });

  it("finds models and names unknown ids", () => {
    expect(findModel(lists, "anthropic/claude-opus-5-5")?.label).toBe("claude-opus-5-5");
    expect(modelNameFromId("lmstudio/qwen/qwen3-14b")).toBe("qwen/qwen3-14b");
    expect(modelNameFromId("plain")).toBe("plain");
  });
});

describe("effectiveEffort", () => {
  it("uses the level asked for, or the nearest lower one the model has", () => {
    const caps = { effort: ["low", "medium", "high"] as const };
    expect(effectiveEffort({ effort: [...caps.effort] }, null)).toBeNull();
    expect(effectiveEffort({ effort: [...caps.effort] }, "medium")).toBe("medium");
    expect(effectiveEffort({ effort: [...caps.effort] }, "max")).toBe("high");
    expect(effectiveEffort({ effort: ["high", "max"] }, "low")).toBe("high");
    expect(effectiveEffort({ effort: [] }, "low")).toBeNull();
  });
});
