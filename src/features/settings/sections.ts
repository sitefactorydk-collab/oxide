import {
  AppWindow,
  Brain,
  Database,
  Globe,
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
    blurb:
      "The models each provider offers, and how Oxide uses them. Only models with native tool calling can be picked.",
  },
  {
    id: "connectors",
    label: "Connectors",
    icon: Plug,
    blurb:
      "Local and remote MCP servers: what they offer, whether they're running, and a place to try their tools.",
  },
  {
    id: "web-search",
    label: "Web search",
    icon: Globe,
    blurb:
      "The service Oxide’s web search tool uses. Anthropic models can use Anthropic’s own search instead. Keys live in the system keychain.",
  },
  {
    id: "desktop",
    label: "Desktop",
    icon: AppWindow,
    blurb: "The tray, quick entry and its shortcut, starting at login, and notifications.",
  },
  {
    id: "extensions",
    label: "Extensions",
    icon: Puzzle,
    blurb:
      "Desktop extensions (.mcpb) bundle an MCP server you can install in one step. Settings marked sensitive are kept in the system keychain.",
  },
  {
    id: "skills",
    label: "Skills",
    icon: Sparkles,
    blurb:
      "Instructions for particular kinds of tasks. Models see each skill’s name and description and load the rest when a task calls for it.",
  },
  {
    id: "permissions",
    label: "Permissions",
    icon: ShieldCheck,
    blurb: "When Oxide asks before a tool runs, and how many tool steps a reply may take.",
  },
  {
    id: "memory",
    label: "Memory",
    icon: Brain,
    blurb:
      "What Oxide remembers across chats (off by default), and how much project knowledge goes into a chat in full.",
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
    blurb: "The MCP config file and every server's log.",
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
