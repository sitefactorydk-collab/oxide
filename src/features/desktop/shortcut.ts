/**
 * Global shortcuts as the backend stores them: modifiers (`Ctrl`, `Alt`, `Shift`, `Super`,
 * `CmdOrCtrl`) then one key, joined by `+` (e.g. `Alt+Space`). Keys use the DOM
 * `KeyboardEvent.code` names that Tauri's shortcut parser accepts.
 */

export const DEFAULT_QUICK_ENTRY_SHORTCUT = "Alt+Space";

const MODIFIER_CODES = new Set([
  "ControlLeft",
  "ControlRight",
  "AltLeft",
  "AltRight",
  "ShiftLeft",
  "ShiftRight",
  "MetaLeft",
  "MetaRight",
  "OSLeft",
  "OSRight",
]);

const NAMED_KEYS = new Set([
  "Space",
  "Enter",
  "Tab",
  "Backspace",
  "Delete",
  "Insert",
  "Home",
  "End",
  "PageUp",
  "PageDown",
  "ArrowUp",
  "ArrowDown",
  "ArrowLeft",
  "ArrowRight",
  "Backquote",
  "Minus",
  "Equal",
  "BracketLeft",
  "BracketRight",
  "Backslash",
  "Semicolon",
  "Quote",
  "Comma",
  "Period",
  "Slash",
]);

function keyName(code: string): string | null {
  if (/^Key[A-Z]$/.test(code)) return code.slice(3);
  if (/^Digit[0-9]$/.test(code)) return code.slice(5);
  if (/^F([1-9]|1[0-9]|2[0-4])$/.test(code)) return code;
  if (NAMED_KEYS.has(code)) return code;
  return null;
}

export type ShortcutKeys = Pick<
  KeyboardEvent,
  "code" | "ctrlKey" | "altKey" | "shiftKey" | "metaKey"
>;

/**
 * The shortcut a key press makes, or `null` while only modifiers are held or when it
 * can't be a global shortcut (no modifier, or only Shift, which would block typing).
 */
export function shortcutFromKeys(e: ShortcutKeys): string | null {
  if (MODIFIER_CODES.has(e.code)) return null;
  const key = keyName(e.code);
  if (!key) return null;
  const mods: string[] = [];
  if (e.ctrlKey) mods.push("Ctrl");
  if (e.altKey) mods.push("Alt");
  if (e.shiftKey) mods.push("Shift");
  if (e.metaKey) mods.push("Super");
  if (mods.length === 0 || (mods.length === 1 && mods[0] === "Shift")) return null;
  return [...mods, key].join("+");
}

const KEY_LABELS: Record<string, string> = {
  ArrowUp: "↑",
  ArrowDown: "↓",
  ArrowLeft: "←",
  ArrowRight: "→",
  Backquote: "`",
  Minus: "-",
  Equal: "=",
  BracketLeft: "[",
  BracketRight: "]",
  Backslash: "\\",
  Semicolon: ";",
  Quote: "'",
  Comma: ",",
  Period: ".",
  Slash: "/",
};

/** How a shortcut is shown: `⌥ Space` style on macOS, `Alt + Space` elsewhere. */
export function formatShortcut(shortcut: string, mac: boolean): string {
  const parts = shortcut.split("+").map((p) => {
    if (mac) {
      switch (p) {
        case "Ctrl":
          return "⌃";
        case "Alt":
          return "⌥";
        case "Shift":
          return "⇧";
        case "Super":
        case "CmdOrCtrl":
          return "⌘";
      }
    } else if (p === "Super") return "Win";
    else if (p === "CmdOrCtrl") return "Ctrl";
    return KEY_LABELS[p] ?? p;
  });
  return mac ? parts.join(" ") : parts.join(" + ");
}
