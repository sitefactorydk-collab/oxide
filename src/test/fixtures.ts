import type { ContentBlock, Conversation, Message, Project, ProviderConfig } from "@/bindings";

/** Seed data for tests (ported from Phase 1's mock store). */

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
      createdAt: now - 30 * DAY,
      updatedAt: now - 20 * MIN,
    },
    {
      id: "p2",
      name: "Garden planner",
      description: "Planting calendar and soil notes for the allotment.",
      instructions: "Use metric units and Danish growing seasons.",
      createdAt: now - 30 * DAY,
      updatedAt: now - 3 * DAY,
    },
  ];
}

function convo(
  id: string,
  title: string,
  model: string,
  updatedAgo: number,
  now: number,
  extra: Partial<Conversation> = {},
): Conversation {
  return {
    id,
    title,
    projectId: null,
    model,
    starred: false,
    createdAt: now - updatedAgo - HOUR,
    updatedAt: now - updatedAgo,
    ...extra,
  };
}

export function seedConversations(now: number): Conversation[] {
  return [
    convo("c1", "Tauri custom title bar on Windows", "anthropic/sonnet", 8 * MIN, now, {
      projectId: "p1",
      starred: true,
    }),
    convo("c2", "Design tokens for light and dark", "anthropic/opus", 2 * HOUR, now, {
      projectId: "p1",
      starred: true,
    }),
    convo("c3", "When to sow broad beans", "openai/gpt", 5 * HOUR, now, { projectId: "p2" }),
    convo("c4", "Explain Rust lifetimes simply", "anthropic/sonnet", 26 * HOUR, now),
    convo("c5", "SQLite FTS5 vs tantivy", "lmstudio/qwen3-14b", 3 * DAY, now),
  ];
}

function message(
  id: string,
  conversationId: string,
  parentId: string | null,
  role: Message["role"],
  createdAt: number,
  content: ContentBlock[],
): Message {
  return { id, conversationId, parentId, role, content, model: null, createdAt };
}

const text = (t: string): ContentBlock => ({ type: "text", text: t });

export function seedMessages(now: number): Message[] {
  return [
    message("m1", "c1", null, "user", now - 10 * MIN, [
      text(
        "How do I draw my own title bar in Tauri v2 on Windows but keep native decorations on Linux?",
      ),
    ]),
    message("m2", "c1", "m1", "assistant", now - 9 * MIN, [
      {
        type: "thinking",
        text: "The user wants per-platform window chrome. Tauri merges platform config files over tauri.conf.json, so decorations can be turned off only on Windows.",
        signature: null,
        durationMs: 6000,
      },
      text(
        'Tauri merges a platform file such as tauri.windows.conf.json over the base config, so you can switch decorations off only where you draw your own bar:\n\n```json\n{ "app": { "windows": [{ "label": "main", "decorations": false }] } }\n```\n\nGrant core:window:allow-start-dragging and the control permissions in your capability file.',
      ),
    ]),
    message("m3", "c1", "m2", "user", now - 8 * MIN, [
      text("Does double-click to maximize still work?"),
    ]),
    message("m4", "c1", "m3", "assistant", now - 8 * MIN, [
      text(
        "Yes. Double-clicking a drag region toggles maximize, as long as the capability also allows core:window:allow-internal-toggle-maximize.",
      ),
    ]),
    message("m5", "c2", null, "user", now - 2 * HOUR, [
      text("Suggest a warm neutral palette with a rust accent."),
    ]),
    message("m6", "c2", "m5", "assistant", now - 2 * HOUR, [
      text(
        "Start from an off-white surface, a slightly darker sidebar, and an iron-oxide accent that holds contrast in both themes.",
      ),
    ]),
  ];
}

export function seedProviders(now: number): ProviderConfig[] {
  const p = (id: ProviderConfig["kind"], name: string, baseUrl: string): ProviderConfig => ({
    id,
    kind: id,
    name,
    baseUrl,
    enabled: true,
    keyHint: null,
    createdAt: now,
    updatedAt: now,
  });
  return [
    p("anthropic", "Anthropic", "https://api.anthropic.com/v1"),
    p("openai", "OpenAI", "https://api.openai.com/v1"),
    p("lmstudio", "LM Studio", "http://127.0.0.1:1234/v1"),
    p("strata", "Strata", "http://127.0.0.1:47831/v1"),
  ];
}
