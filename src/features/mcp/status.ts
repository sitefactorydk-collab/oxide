import type { McpServerState, McpStatus } from "@/bindings";

export const STATUS_LABEL: Record<McpStatus, string> = {
  disabled: "Off",
  starting: "Starting",
  running: "Running",
  error: "Error",
  needsAuth: "Sign-in needed",
  stopped: "Stopped",
  needsTrust: "Waiting for your OK",
};

/** "6 tools · 1 resource · 2 prompts" (only what the server has). */
export function countsLabel(s: McpServerState): string {
  const part = (n: number, one: string) => `${n} ${one}${n === 1 ? "" : "s"}`;
  const parts: string[] = [];
  if (s.capabilities.tools || s.tools > 0) parts.push(part(s.tools, "tool"));
  if (s.capabilities.resources || s.resources > 0) parts.push(part(s.resources, "resource"));
  if (s.capabilities.prompts || s.prompts > 0) parts.push(part(s.prompts, "prompt"));
  return parts.join(" · ");
}

export function protocolLabel(s: McpServerState): string | null {
  if (!s.protocolVersion) return null;
  return s.era === "legacy" ? `MCP ${s.protocolVersion} (legacy)` : `MCP ${s.protocolVersion}`;
}
