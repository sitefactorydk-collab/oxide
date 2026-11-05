import { modKeyLabel } from "@/lib/platform";

export type ShortcutId = "search" | "newChat" | "shortcuts" | "toggleSidebar" | "settings";

export interface Shortcut {
  id: ShortcutId;
  label: string;
  /** Keys after the platform modifier. */
  keys: string[];
}

export const SHORTCUTS: Shortcut[] = [
  { id: "search", label: "Search chats", keys: ["K"] },
  { id: "newChat", label: "New chat", keys: ["Shift", "O"] },
  { id: "toggleSidebar", label: "Toggle sidebar", keys: ["B"] },
  { id: "settings", label: "Open settings", keys: [","] },
  { id: "shortcuts", label: "Show keyboard shortcuts", keys: ["/"] },
];

/** Static composer hints shown in the shortcuts dialog (no modifier). */
export const COMPOSER_HINTS: Array<{ label: string; keys: string[] }> = [
  { label: "Send message", keys: ["Enter"] },
  { label: "New line", keys: ["Shift", "Enter"] },
];

export function shortcutKeys(id: ShortcutId): string[] {
  const s = SHORTCUTS.find((x) => x.id === id);
  return s ? [modKeyLabel, ...s.keys] : [];
}

/** Text form for tooltips and menus, e.g. "Ctrl+Shift+O" or "⌘⇧O". */
export function shortcutText(id: ShortcutId): string {
  const keys = shortcutKeys(id);
  if (modKeyLabel === "⌘") return keys.map((k) => (k === "Shift" ? "⇧" : k)).join("");
  return keys.join("+");
}

type KeyLike = Pick<KeyboardEvent, "key" | "code" | "ctrlKey" | "metaKey" | "shiftKey" | "altKey">;

/** Maps a keydown to a shortcut. `mac` selects ⌘ vs Ctrl as the modifier. */
export function matchShortcut(e: KeyLike, mac: boolean): ShortcutId | null {
  const mod = mac ? e.metaKey && !e.ctrlKey : e.ctrlKey && !e.metaKey;
  if (!mod || e.altKey) return null;
  const key = e.key.toLowerCase();
  if (e.shiftKey && (key === "o" || e.code === "KeyO")) return "newChat";
  // "/" sits on Shift+7 on Nordic/German layouts, so accept it with or without Shift.
  if (key === "/" || e.code === "Slash" || e.code === "NumpadDivide") return "shortcuts";
  if (e.shiftKey) return null;
  if (key === "k" || e.code === "KeyK") return "search";
  if (key === "b" || e.code === "KeyB") return "toggleSidebar";
  if (key === "," || e.code === "Comma") return "settings";
  return null;
}
