/**
 * Interface language (Phase 18): English and Danish catalogs and `useT()`.
 *
 * The setting lives in Settings → Appearance (`appearance.language`: system, en, da) and is
 * cached in `useUi` for the first frame like the theme. "System" follows the web view's
 * language (`navigator.language`, which is the system's). Strings not moved to the catalog
 * yet stay English.
 */
import { useEffect } from "react";
import type { UiLanguage } from "@/bindings";
import { useUi } from "@/stores/ui";
import { da } from "./da";
import { en, type Catalog, type MessageKey } from "./en";

export type { MessageKey };
export type Lang = "en" | "da";

const CATALOGS: Record<Lang, Catalog> = { en, da };

/** The language to show for a preference. */
export function resolveLanguage(
  pref: UiLanguage,
  system: string | undefined = typeof navigator === "undefined" ? undefined : navigator.language,
): Lang {
  if (pref === "en" || pref === "da") return pref;
  const tag = (system ?? "").toLowerCase();
  return tag === "da" || tag.startsWith("da-") ? "da" : "en";
}

/** Fills `{name}` placeholders. */
function fill(text: string, vars?: Record<string, string | number>): string {
  if (!vars) return text;
  return text.replace(/\{(\w+)\}/g, (m, k: string) => (k in vars ? String(vars[k]) : m));
}

export function translate(
  lang: Lang,
  key: MessageKey,
  vars?: Record<string, string | number>,
): string {
  return fill(CATALOGS[lang][key] ?? en[key] ?? key, vars);
}

export type T = (key: MessageKey, vars?: Record<string, string | number>) => string;

export function useLang(): Lang {
  const pref = useUi((s) => s.language);
  return resolveLanguage(pref);
}

/** The translate function for the current interface language. */
export function useT(): T {
  const lang = useLang();
  return (key, vars) => translate(lang, key, vars);
}

/** Keeps `<html lang>` in step with the interface language (mounted once in the shell). */
export function useDocumentLanguage(): void {
  const lang = useLang();
  useEffect(() => {
    document.documentElement.lang = lang;
  }, [lang]);
}
