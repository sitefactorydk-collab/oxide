import {
  Brain,
  Database,
  Info,
  KeyRound,
  Palette,
  Plug,
  Puzzle,
  Server,
  ShieldCheck,
  SlidersHorizontal,
  Sparkles,
  Terminal,
  UserRound,
  type LucideIcon,
} from "lucide-react";

export interface SettingsSection {
  id: string;
  label: string;
  icon: LucideIcon;
  /** Phase that makes the section real (shown on placeholder pages). */
  phase?: number;
  blurb: string;
}

export const SETTINGS_SECTIONS: SettingsSection[] = [
  {
    id: "general",
    label: "General",
    icon: UserRound,
    blurb: "Your name and how Oxide greets you.",
  },
  { id: "appearance", label: "Appearance", icon: Palette, blurb: "Theme and chat font." },
  {
    id: "providers",
    label: "Providers",
    icon: KeyRound,
    blurb:
      "Endpoints and API keys for Anthropic, OpenAI, LM Studio and Strata. Keys live in the system keychain.",
  },
  {
    id: "models",
    label: "Models",
    icon: SlidersHorizontal,
    phase: 3,
    blurb: "Which models appear in the picker. Only models with native tool calling are listed.",
  },
  {
    id: "connectors",
    label: "Connectors",
    icon: Plug,
    phase: 5,
    blurb: "Local and remote MCP servers, with import from an existing desktop config.",
  },
  {
    id: "extensions",
    label: "Extensions",
    icon: Puzzle,
    phase: 10,
    blurb: "Install and configure desktop extensions (.mcpb).",
  },
  {
    id: "skills",
    label: "Skills",
    icon: Sparkles,
    phase: 10,
    blurb: "Folders of instructions the model can load on demand.",
  },
  {
    id: "permissions",
    label: "Permissions",
    icon: ShieldCheck,
    phase: 6,
    blurb: "Tool approvals, auto-allow rules and folder access.",
  },
  {
    id: "memory",
    label: "Memory",
    icon: Brain,
    phase: 7,
    blurb: "What Oxide remembers across chats. Off by default.",
  },
  {
    id: "data",
    label: "Data",
    icon: Database,
    phase: 17,
    blurb: "Export everything or delete all local data.",
  },
  {
    id: "developer",
    label: "Developer",
    icon: Terminal,
    phase: 5,
    blurb: "Config file, server logs and debug tools.",
  },
  {
    id: "advanced",
    label: "Advanced",
    icon: Server,
    phase: 12,
    blurb: "Managed Python environment: repair and reset.",
  },
  { id: "about", label: "About", icon: Info, blurb: "Version information." },
];

export function findSection(id: string | undefined): SettingsSection {
  const fallback = SETTINGS_SECTIONS[0] as SettingsSection;
  return SETTINGS_SECTIONS.find((s) => s.id === id) ?? fallback;
}
