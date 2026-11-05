import type { HighlighterCore, ThemedToken } from "shiki/core";

/**
 * Syntax highlighting with Shiki, loaded on first use.
 *
 * - The JavaScript regex engine is used instead of Oniguruma's WASM, which the app's CSP
 *   (`script-src 'self'`) wouldn't allow.
 * - Colours come from a CSS-variables theme, so tokens use `--ox-shiki-*` from tokens.css and
 *   follow light/dark mode without re-highlighting.
 * - Grammars load per language on demand (Vite splits each into its own chunk).
 * - Results are cached by language and code, so re-rendering a message (or scrolling back to
 *   it) shows colours at once.
 */

export type Tokens = ThemedToken[][];

const THEME = "oxide";
const CACHE_LIMIT = 300;

/** Common names models use that Shiki spells differently. */
const ALIASES: Record<string, string> = {
  "c++": "cpp",
  "c#": "csharp",
  "f#": "fsharp",
  golang: "go",
  "objective-c": "objc",
  plaintext: "text",
  console: "shellsession",
  ps: "powershell",
  pwsh: "powershell",
  shell: "bash",
  sh: "bash",
  zsh: "bash",
  vue: "vue",
};

let highlighter: Promise<HighlighterCore> | null = null;
const cache = new Map<string, Tokens>();
const failed = new Set<string>();

function getHighlighter(): Promise<HighlighterCore> {
  highlighter ??= (async () => {
    const [{ createHighlighterCore, createCssVariablesTheme }, { createJavaScriptRegexEngine }] =
      await Promise.all([import("shiki/core"), import("shiki/engine/javascript")]);
    return createHighlighterCore({
      themes: [
        createCssVariablesTheme({
          name: THEME,
          variablePrefix: "--ox-shiki-",
          variableDefaults: {},
          fontStyle: true,
        }),
      ],
      langs: [],
      engine: createJavaScriptRegexEngine({ forgiving: true }),
    });
  })();
  return highlighter;
}

/** The Shiki language id for a fence's info string, or null for plain text. */
export function normalizeLang(lang: string): string {
  const l = lang.trim().toLowerCase().split(/[\s{]/)[0] ?? "";
  return ALIASES[l] ?? l;
}

function key(lang: string, code: string): string {
  return `${lang}\u0000${code}`;
}

/** Cached tokens, if this exact code was highlighted before. */
export function cachedTokens(lang: string, code: string): Tokens | undefined {
  return cache.get(key(normalizeLang(lang), code));
}

/**
 * Highlights `code`. Resolves to `null` for unknown languages or when highlighting fails;
 * callers then keep showing plain text.
 */
export async function highlight(lang: string, code: string): Promise<Tokens | null> {
  const id = normalizeLang(lang);
  if (!id || id === "text" || failed.has(id)) return null;
  const k = key(id, code);
  const hit = cache.get(k);
  if (hit) return hit;
  try {
    const { bundledLanguages } = await import("shiki/langs");
    const loader = (bundledLanguages as Record<string, unknown>)[id];
    if (!loader) {
      failed.add(id);
      return null;
    }
    const h = await getHighlighter();
    if (!h.getLoadedLanguages().includes(id)) {
      await h.loadLanguage(loader as Parameters<HighlighterCore["loadLanguage"]>[0]);
    }
    const tokens = h.codeToTokens(code, { lang: id, theme: THEME }).tokens;
    cache.set(k, tokens);
    if (cache.size > CACHE_LIMIT) {
      const oldest = cache.keys().next().value;
      if (oldest !== undefined) cache.delete(oldest);
    }
    return tokens;
  } catch {
    failed.add(id);
    return null;
  }
}
