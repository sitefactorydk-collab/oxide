import { commands } from "@/bindings";

/** Whether a link can be opened outside the app (web and mail links only). */
export function isExternalUrl(href: string | undefined): href is string {
  return !!href && /^(https?:|mailto:)/i.test(href.trim());
}

/**
 * Opens a web or mail link in the system browser or mail app. Links must never navigate the
 * app's own window.
 */
export function openExternal(href: string): void {
  if (!isExternalUrl(href)) return;
  void commands.openExternal(href.trim()).catch(() => {});
}
