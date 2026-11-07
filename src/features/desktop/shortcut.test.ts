import { describe, expect, it } from "vitest";
import { formatShortcut, shortcutFromKeys, type ShortcutKeys } from "./shortcut";

const keys = (code: string, mods: Partial<ShortcutKeys> = {}): ShortcutKeys => ({
  code,
  ctrlKey: false,
  altKey: false,
  shiftKey: false,
  metaKey: false,
  ...mods,
});

describe("shortcutFromKeys", () => {
  it("builds shortcuts in the backend's form", () => {
    expect(shortcutFromKeys(keys("Space", { altKey: true }))).toBe("Alt+Space");
    expect(shortcutFromKeys(keys("KeyK", { ctrlKey: true, shiftKey: true }))).toBe("Ctrl+Shift+K");
    expect(shortcutFromKeys(keys("Digit1", { metaKey: true }))).toBe("Super+1");
    expect(shortcutFromKeys(keys("F5", { ctrlKey: true, altKey: true }))).toBe("Ctrl+Alt+F5");
    expect(shortcutFromKeys(keys("ArrowUp", { ctrlKey: true }))).toBe("Ctrl+ArrowUp");
    expect(shortcutFromKeys(keys("Period", { altKey: true }))).toBe("Alt+Period");
  });

  it("waits while only modifiers are held and refuses unsafe ones", () => {
    expect(shortcutFromKeys(keys("AltLeft", { altKey: true }))).toBeNull();
    expect(shortcutFromKeys(keys("KeyA"))).toBeNull();
    expect(shortcutFromKeys(keys("KeyA", { shiftKey: true }))).toBeNull();
    expect(shortcutFromKeys(keys("IntlBackslash", { ctrlKey: true }))).toBeNull();
    expect(shortcutFromKeys(keys("F25", { ctrlKey: true }))).toBeNull();
  });
});

describe("formatShortcut", () => {
  it("uses symbols on macOS and words elsewhere", () => {
    expect(formatShortcut("Alt+Space", true)).toBe("⌥ Space");
    expect(formatShortcut("Ctrl+Shift+Super+K", true)).toBe("⌃ ⇧ ⌘ K");
    expect(formatShortcut("Alt+Space", false)).toBe("Alt + Space");
    expect(formatShortcut("Super+ArrowUp", false)).toBe("Win + ↑");
    expect(formatShortcut("CmdOrCtrl+Slash", false)).toBe("Ctrl + /");
  });
});
