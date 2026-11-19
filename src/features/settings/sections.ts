import {
  AppWindow,
  BarChart3,
  Box,
  Brain,
  Compass,
  Database,
  Globe,
  Info,
  KeyRound,
  Mic,
  MonitorSmartphone,
  PanelTop,
  Palette,
  Plug,
  Puzzle,
  Server,
  ShieldCheck,
  SlidersHorizontal,
  Sparkles,
  Terminal,
  UserRound,
  Wand2,
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
    id: "generation",
    label: "Media generation",
    icon: Wand2,
    blurb:
      "The models that make images, video and music, on OpenAI-compatible endpoints. Chats use them through their generation tools.",
  },
  {
    id: "voice",
    label: "Voice",
    icon: Mic,
    blurb:
      "Dictation: talk instead of typing. Speech goes to an OpenAI-compatible speech-to-text endpoint you pick.",
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
    id: "sandbox",
    label: "Sandbox",
    icon: Box,
    blurb:
      "An isolated Docker container per chat, where models can build, run and test code without touching your computer.",
  },
  {
    id: "browser",
    label: "Browser",
    icon: Compass,
    blurb:
      "A web browser models can open, read, click and type in, and an autonomous browsing agent for whole tasks.",
  },
  {
    id: "chrome",
    label: "Chrome",
    icon: PanelTop,
    blurb:
      "Oxide's Chrome extension: pair your everyday Chrome so models can work in its tabs, with your sign-ins.",
  },
  {
    id: "computer",
    label: "Computer use",
    icon: MonitorSmartphone,
    blurb:
      "Let models see your screen and use the mouse and keyboard, like a person at your computer. Each chat asks first, and so does each app it uses.",
  },
  {
    id: "memory",
    label: "Memory",
    icon: Brain,
    blurb:
      "What Oxide remembers across chats (off by default), and how much project knowledge goes into a chat in full.",
  },
  {
    id: "usage",
    label: "Usage",
    icon: BarChart3,
    blurb: "Tokens your chats used, per provider, model and day, as the providers reported them.",
  },
  {
    id: "data",
    label: "Data",
    icon: Database,
    blurb: "Where Oxide keeps your data, a full export, and deleting everything.",
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
    blurb: "Managed Python environment: repair and reset.",
  },
  { id: "about", label: "About", icon: Info, blurb: "Version information and updates." },
];

export function findSection(id: string | undefined): SettingsSection {
  const fallback = SETTINGS_SECTIONS[0] as SettingsSection;
  return SETTINGS_SECTIONS.find((s) => s.id === id) ?? fallback;
}
