/**
 * English strings of Oxide's own interface (Phase 18). Keys are grouped by where they show;
 * `{name}` placeholders are filled by `t(key, { name })`. Add a key here first: the Danish
 * catalog must then have it too (the type checker says so).
 */
export const en = {
  // Sidebar
  "nav.newChat": "New chat",
  "nav.search": "Search",
  "nav.searchChats": "Search chats",
  "nav.chats": "Chats",
  "nav.projects": "Projects",
  "nav.artifacts": "Artifacts",
  "nav.scheduled": "Scheduled",
  "nav.starred": "Starred",
  "nav.recents": "Recents",
  "nav.noChats": "No chats yet",
  "nav.sidebar": "Sidebar",
  // Profile menu
  "profile.local": "Local profile",
  "profile.menu": "Profile and settings",
  "profile.you": "You",
  "menu.settings": "Settings",
  "menu.appearance": "Appearance",
  "menu.light": "Light",
  "menu.dark": "Dark",
  "menu.system": "Match system",
  "menu.moreAppearance": "More appearance settings…",
  "menu.help": "Help",
  "menu.shortcuts": "Keyboard shortcuts",
  "menu.about": "About Oxide",
  // Home
  "home.morning": "Good morning",
  "home.afternoon": "Good afternoon",
  "home.evening": "Good evening",
  "home.late": "Up late",
  "home.chip.draft": "Draft",
  "home.chip.explain": "Explain",
  "home.chip.code": "Code",
  "home.chip.plan": "Plan",
  "home.chip.summarize": "Summarize",
  "home.startFailed": "Couldn’t start the chat: {error}",
  // Composer
  "composer.homePlaceholder": "What should we work on?",
  "composer.replyPlaceholder": "Reply…",
  "composer.message": "Message",
  "composer.send": "Send",
  "composer.stop": "Stop",
  "composer.dictate": "Dictate",
  "composer.stopDictation": "Stop dictation",
  "composer.transcribing": "Transcribing…",
  // Settings
  "settings.title": "Settings",
  "settings.sections": "Settings sections",
  "settings.close": "Close settings",
  "settings.language": "Language",
  "settings.languageHint":
    "The language of Oxide’s menus and buttons. Models answer in whatever language you write.",
  "settings.languageSystem": "Match system",
  "section.general": "General",
  "section.appearance": "Appearance",
  "section.providers": "Providers",
  "section.models": "Models",
  "section.connectors": "Connectors",
  "section.web-search": "Web search",
  "section.voice": "Voice",
  "section.generation": "Generation",
  "section.desktop": "Desktop",
  "section.extensions": "Extensions",
  "section.skills": "Skills",
  "section.permissions": "Permissions",
  "section.sandbox": "Sandbox",
  "section.browser": "Browser",
  "section.chrome": "Chrome",
  "section.computer": "Computer use",
  "section.memory": "Memory",
  "section.usage": "Usage",
  "section.data": "Data",
  "section.developer": "Developer",
  "section.advanced": "Advanced",
  "section.about": "About",
  // Common
  "common.cancel": "Cancel",
  "common.save": "Save",
  "common.close": "Close",
} as const;

export type MessageKey = keyof typeof en;
export type Catalog = Record<MessageKey, string>;
