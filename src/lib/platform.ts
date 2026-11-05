import { isTauri } from "@tauri-apps/api/core";

/** Where the UI is running. `web` = plain browser (e.g. `npm run dev` opened in a browser, or tests). */
export type Platform = "windows" | "macos" | "linux" | "web";

export function detectPlatform(userAgent: string, inTauri: boolean): Platform {
  if (!inTauri) return "web";
  if (/Windows/i.test(userAgent)) return "windows";
  if (/Macintosh|Mac OS X/i.test(userAgent)) return "macos";
  return "linux";
}

function safeIsTauri(): boolean {
  try {
    return isTauri();
  } catch {
    return false;
  }
}

const userAgent = typeof navigator === "undefined" ? "" : navigator.userAgent;

export const inTauri = safeIsTauri();
export const platform: Platform = detectPlatform(userAgent, inTauri);

/** Shortcut modifier is ⌘ on Apple platforms, Ctrl elsewhere (also in a browser on a Mac). */
export const isMacLike = /Macintosh|Mac OS X|iPhone|iPad/i.test(userAgent);
export const modKeyLabel = isMacLike ? "⌘" : "Ctrl";
