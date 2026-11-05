import type { Artifact, Conversation, Message, ModelOption, Project } from "./types";

/** Placeholder model list. Phase 3 replaces it with per-provider discovery + capability checks. */
export const MOCK_MODELS: ModelOption[] = [
  { id: "anthropic/opus", label: "Opus", provider: "Anthropic", note: "Most capable" },
  { id: "anthropic/sonnet", label: "Sonnet", provider: "Anthropic", note: "Balanced" },
  { id: "anthropic/haiku", label: "Haiku", provider: "Anthropic", note: "Fastest" },
  { id: "openai/gpt", label: "GPT", provider: "OpenAI" },
  { id: "openai/gpt-mini", label: "GPT mini", provider: "OpenAI" },
  { id: "lmstudio/qwen3-14b", label: "qwen3-14b", provider: "LM Studio", note: "Local" },
  { id: "strata/default", label: "strata-default", provider: "Strata", note: "Local" },
];

export function modelLabel(id: string): string {
  return MOCK_MODELS.find((m) => m.id === id)?.label ?? id;
}

const MIN = 60_000;
const HOUR = 60 * MIN;
const DAY = 24 * HOUR;

export function seedProjects(now: number): Project[] {
  return [
    {
      id: "p1",
      name: "Oxide",
      description: "A Rust remake of a desktop AI assistant.",
      instructions:
        "Answer as a senior Rust + TypeScript engineer. Prefer small, verifiable steps and keep crate boundaries from CLAUDE.md.",
      knowledge: [
        { name: "PLAN.md", bytes: 29_353 },
        { name: "PROGRESS.md", bytes: 6_162 },
        { name: "CLAUDE.md", bytes: 3_557 },
      ],
      updatedAt: now - 20 * MIN,
    },
    {
      id: "p2",
      name: "Garden planner",
      description: "Planting calendar and soil notes for the allotment.",
      instructions: "Use metric units and Danish growing seasons.",
      knowledge: [{ name: "soil-test-2026.pdf", bytes: 412_000 }],
      updatedAt: now - 3 * DAY,
    },
    {
      id: "p3",
      name: "Reading notes",
      description: "Summaries and quotes from books in progress.",
      instructions: "Keep summaries under 200 words.",
      knowledge: [],
      updatedAt: now - 12 * DAY,
    },
  ];
}

export function seedConversations(now: number): Conversation[] {
  return [
    {
      id: "c1",
      title: "Tauri custom title bar on Windows",
      projectId: "p1",
      modelId: "anthropic/sonnet",
      starred: true,
      updatedAt: now - 8 * MIN,
    },
    {
      id: "c2",
      title: "Design tokens for light and dark",
      projectId: "p1",
      modelId: "anthropic/opus",
      starred: true,
      updatedAt: now - 2 * HOUR,
    },
    {
      id: "c3",
      title: "When to sow broad beans",
      projectId: "p2",
      modelId: "openai/gpt",
      starred: false,
      updatedAt: now - 5 * HOUR,
    },
    {
      id: "c4",
      title: "Explain Rust lifetimes simply",
      modelId: "anthropic/sonnet",
      starred: false,
      updatedAt: now - 26 * HOUR,
    },
    {
      id: "c5",
      title: "SQLite FTS5 vs tantivy",
      modelId: "lmstudio/qwen3-14b",
      starred: false,
      updatedAt: now - 3 * DAY,
    },
    {
      id: "c6",
      title: "Weekend trip packing list",
      modelId: "anthropic/haiku",
      starred: false,
      updatedAt: now - 6 * DAY,
    },
    {
      id: "c7",
      title: "Regex for Danish phone numbers",
      modelId: "strata/default",
      starred: false,
      updatedAt: now - 9 * DAY,
    },
  ];
}

export function seedMessages(now: number): Record<string, Message[]> {
  return {
    c1: [
      {
        id: "m1",
        role: "user",
        createdAt: now - 10 * MIN,
        blocks: [
          {
            type: "p",
            text: "How do I draw my own title bar in Tauri v2 on Windows but keep native decorations on Linux?",
          },
        ],
      },
      {
        id: "m2",
        role: "assistant",
        createdAt: now - 9 * MIN,
        thinking: {
          seconds: 6,
          text: "The user wants per-platform window chrome. Tauri merges platform config files over tauri.conf.json, so decorations can be turned off only on Windows. The custom bar needs a drag region and window-control permissions in the capability file.",
        },
        blocks: [
          {
            type: "p",
            text: "Tauri merges a platform file such as tauri.windows.conf.json over the base config, so you can switch decorations off only where you draw your own bar:",
          },
          {
            type: "code",
            lang: "json",
            code: '{\n  "app": {\n    "windows": [\n      { "label": "main", "decorations": false }\n    ]\n  }\n}',
          },
          {
            type: "p",
            text: "Then render a bar in the webview and mark its empty area as draggable:",
          },
          {
            type: "ul",
            items: [
              "Add data-tauri-drag-region to the bar and any spacer inside it.",
              "Call minimize(), toggleMaximize() and close() from @tauri-apps/api/window.",
              "Grant core:window:allow-start-dragging and the three control permissions in your capability file.",
            ],
          },
          {
            type: "p",
            text: "On Linux leave decorations on, and the native title bar keeps working with the system theme.",
          },
        ],
      },
      {
        id: "m3",
        role: "user",
        createdAt: now - 8 * MIN,
        blocks: [{ type: "p", text: "Does double-click to maximize still work?" }],
      },
      {
        id: "m4",
        role: "assistant",
        createdAt: now - 8 * MIN,
        blocks: [
          {
            type: "p",
            text: "Yes. Double-clicking a drag region toggles maximize, as long as the capability also allows core:window:allow-internal-toggle-maximize.",
          },
        ],
      },
    ],
    c2: [
      {
        id: "m5",
        role: "user",
        createdAt: now - 2 * HOUR,
        blocks: [{ type: "p", text: "Suggest a warm neutral palette with a rust accent." }],
      },
      {
        id: "m6",
        role: "assistant",
        createdAt: now - 2 * HOUR,
        blocks: [
          {
            type: "p",
            text: "Start from an off-white surface, a slightly darker sidebar, and an iron-oxide accent that holds contrast in both themes.",
          },
        ],
      },
    ],
  };
}

export function seedArtifacts(now: number): Artifact[] {
  return [
    {
      id: "a1",
      title: "Title bar component",
      kind: "react",
      conversationId: "c1",
      updatedAt: now - 9 * MIN,
    },
    {
      id: "a2",
      title: "Palette swatches",
      kind: "html",
      conversationId: "c2",
      updatedAt: now - 2 * HOUR,
    },
    {
      id: "a3",
      title: "Sowing calendar",
      kind: "markdown",
      conversationId: "c3",
      updatedAt: now - 5 * HOUR,
    },
    {
      id: "a4",
      title: "Crate dependency graph",
      kind: "mermaid",
      conversationId: "c5",
      updatedAt: now - 3 * DAY,
    },
    { id: "a5", title: "Oxide mark", kind: "svg", conversationId: "c2", updatedAt: now - 4 * DAY },
    {
      id: "a6",
      title: "Phone number regex",
      kind: "code",
      conversationId: "c7",
      updatedAt: now - 9 * DAY,
    },
  ];
}

export const MOCK_REPLY: Message["blocks"] = [
  {
    type: "p",
    text: "This is a placeholder reply. Oxide is running on mock data in this build, so nothing was sent to a model.",
  },
  {
    type: "p",
    text: "Real streaming answers arrive in Phase 3, when Oxide connects to Anthropic, OpenAI, LM Studio and Strata.",
  },
];
