// Text helpers of the builder's code editor.

/** The Shiki language of a file. */
export function languageOf(path: string): string {
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  const map: Record<string, string> = {
    tsx: "tsx",
    ts: "typescript",
    jsx: "jsx",
    js: "javascript",
    mjs: "javascript",
    json: "json",
    css: "css",
    md: "markdown",
    html: "html",
    svg: "xml",
  };
  return map[ext] ?? "text";
}

/** Where `query` occurs in `text` (case-insensitive). */
export function findAll(text: string, query: string): number[] {
  if (!query) return [];
  const hay = text.toLowerCase();
  const needle = query.toLowerCase();
  const out: number[] = [];
  for (let i = hay.indexOf(needle); i !== -1; i = hay.indexOf(needle, i + needle.length)) {
    out.push(i);
    if (out.length >= 5000) break;
  }
  return out;
}
