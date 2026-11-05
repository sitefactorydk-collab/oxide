import { describe, expect, it } from "vitest";
import { matchShortcut } from "@/app/shortcuts";
import { resolveTheme } from "@/app/useThemeSync";
import { detectPlatform } from "@/lib/platform";
import { initials } from "@/lib/profile";
import { formatRelative, greetingFor } from "@/lib/time";
import { clampSidebarWidth, SIDEBAR_DEFAULT, SIDEBAR_MAX, SIDEBAR_MIN } from "@/stores/ui";

const key = (k: string, mods: Partial<KeyboardEvent> = {}) => ({
  key: k,
  code: "",
  ctrlKey: false,
  metaKey: false,
  shiftKey: false,
  altKey: false,
  ...mods,
});

describe("matchShortcut", () => {
  it("uses Ctrl on Windows/Linux and Cmd on macOS", () => {
    expect(matchShortcut(key("k", { ctrlKey: true }), false)).toBe("search");
    expect(matchShortcut(key("k", { metaKey: true }), false)).toBeNull();
    expect(matchShortcut(key("k", { metaKey: true }), true)).toBe("search");
    expect(matchShortcut(key("k", { ctrlKey: true }), true)).toBeNull();
  });

  it("maps every documented shortcut", () => {
    expect(matchShortcut(key("O", { ctrlKey: true, shiftKey: true }), false)).toBe("newChat");
    expect(matchShortcut(key("b", { ctrlKey: true }), false)).toBe("toggleSidebar");
    expect(matchShortcut(key(",", { ctrlKey: true }), false)).toBe("settings");
    expect(matchShortcut(key("/", { ctrlKey: true }), false)).toBe("shortcuts");
  });

  it("accepts / typed as Shift+7 on Nordic layouts", () => {
    expect(matchShortcut(key("/", { ctrlKey: true, shiftKey: true, code: "Digit7" }), false)).toBe(
      "shortcuts",
    );
  });

  it("ignores plain keys and AltGr combos", () => {
    expect(matchShortcut(key("k"), false)).toBeNull();
    expect(matchShortcut(key("b", { ctrlKey: true, altKey: true }), false)).toBeNull();
  });
});

describe("helpers", () => {
  it("clamps the sidebar width", () => {
    expect(clampSidebarWidth(10)).toBe(SIDEBAR_MIN);
    expect(clampSidebarWidth(9999)).toBe(SIDEBAR_MAX);
    expect(clampSidebarWidth(Number.NaN)).toBe(SIDEBAR_DEFAULT);
    expect(clampSidebarWidth(300.4)).toBe(300);
  });

  it("resolves the theme preference", () => {
    expect(resolveTheme("system", true)).toBe("dark");
    expect(resolveTheme("system", false)).toBe("light");
    expect(resolveTheme("light", true)).toBe("light");
  });

  it("detects the platform from the user agent", () => {
    expect(detectPlatform("Mozilla/5.0 (Windows NT 10.0; Win64; x64) Edg/140", true)).toBe(
      "windows",
    );
    expect(detectPlatform("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)", true)).toBe("macos");
    expect(detectPlatform("Mozilla/5.0 (X11; Linux x86_64)", true)).toBe("linux");
    expect(detectPlatform("Mozilla/5.0 (Windows NT 10.0)", false)).toBe("web");
  });

  it("greets by time of day", () => {
    expect(greetingFor(8)).toBe("Good morning");
    expect(greetingFor(14)).toBe("Good afternoon");
    expect(greetingFor(20)).toBe("Good evening");
    expect(greetingFor(2)).toBe("Up late");
  });

  it("formats relative times", () => {
    const now = 1_000_000_000_000;
    expect(formatRelative(now - 5_000, now)).toBe("just now");
    expect(formatRelative(now - 5 * 60_000, now)).toBe("5 min ago");
    expect(formatRelative(now - 3 * 3_600_000, now)).toBe("3 hours ago");
    expect(formatRelative(now - 30 * 3_600_000, now)).toBe("Yesterday");
  });

  it("builds avatar initials", () => {
    expect(initials("Ada Lovelace")).toBe("AL");
    expect(initials("mikkel")).toBe("M");
    expect(initials("  ")).toBe("?");
  });
});
