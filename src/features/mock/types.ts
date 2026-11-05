/**
 * UI-side shapes for Phase 1's mock data. Phase 2 replaces these with the generated
 * `oxide-core` types from `@/bindings`.
 */

export type Block =
  | { type: "p"; text: string }
  | { type: "code"; lang: string; code: string }
  | { type: "ul"; items: string[] };

export interface Thinking {
  seconds: number;
  text: string;
}

export interface Message {
  id: string;
  role: "user" | "assistant";
  blocks: Block[];
  thinking?: Thinking;
  createdAt: number;
}

export interface Conversation {
  id: string;
  title: string;
  projectId?: string;
  modelId: string;
  starred: boolean;
  updatedAt: number;
}

export interface KnowledgeFile {
  name: string;
  bytes: number;
}

export interface Project {
  id: string;
  name: string;
  description: string;
  instructions: string;
  knowledge: KnowledgeFile[];
  updatedAt: number;
}

export type ArtifactKind = "code" | "markdown" | "html" | "svg" | "mermaid" | "react";

export interface Artifact {
  id: string;
  title: string;
  kind: ArtifactKind;
  conversationId: string;
  updatedAt: number;
}

export interface ModelOption {
  id: string;
  label: string;
  provider: "Anthropic" | "OpenAI" | "LM Studio" | "Strata";
  note?: string;
}
