import type { ArtifactKind, ContentBlock } from "@/bindings";

/**
 * The string fields of a JSON object that may still be streaming. Values cut off at the
 * end come back as far as they've arrived; non-string values are skipped. Only the top
 * level is read.
 */
export function partialFields(json: string): Record<string, string> {
  const out: Record<string, string> = {};
  let i = 0;
  const n = json.length;
  const ws = () => {
    while (i < n && /\s/.test(json[i]!)) i++;
  };
  /** Reads a string starting after its opening quote. `done` is false if it was cut off. */
  const str = (): { value: string; done: boolean } => {
    let value = "";
    while (i < n) {
      const c = json[i]!;
      if (c === '"') {
        i++;
        return { value, done: true };
      }
      if (c === "\\") {
        if (i + 1 >= n) {
          i = n;
          break;
        }
        const e = json[i + 1]!;
        if (e === "u") {
          const hex = json.slice(i + 2, i + 6);
          if (hex.length < 4) {
            i = n;
            break;
          }
          value += String.fromCharCode(parseInt(hex, 16) || 0);
          i += 6;
          continue;
        }
        value +=
          e === "n"
            ? "\n"
            : e === "t"
              ? "\t"
              : e === "r"
                ? "\r"
                : e === "b"
                  ? "\b"
                  : e === "f"
                    ? "\f"
                    : e;
        i += 2;
        continue;
      }
      value += c;
      i++;
    }
    return { value, done: false };
  };
  /** Skips a non-string value (number, literal, array, object). */
  const skip = () => {
    let depth = 0;
    while (i < n) {
      const c = json[i]!;
      if (c === '"') {
        i++;
        str();
        continue;
      }
      if (c === "{" || c === "[") depth++;
      else if (c === "}" || c === "]") {
        if (depth === 0) return;
        depth--;
      } else if (c === "," && depth === 0) return;
      i++;
    }
  };
  ws();
  if (json[i] !== "{") return out;
  i++;
  while (i < n) {
    ws();
    if (json[i] === ",") {
      i++;
      continue;
    }
    if (json[i] === "}") break;
    if (json[i] !== '"') break;
    i++;
    const key = str();
    if (!key.done) break;
    ws();
    if (json[i] !== ":") break;
    i++;
    ws();
    if (i >= n) break;
    if (json[i] === '"') {
      i++;
      const v = str();
      out[key.value] = v.value;
      if (!v.done) break;
    } else {
      skip();
    }
  }
  return out;
}

const KINDS: Record<string, ArtifactKind> = {
  markdown: "markdown",
  md: "markdown",
  document: "markdown",
  code: "code",
  html: "html",
  svg: "svg",
  mermaid: "mermaid",
  react: "react",
  jsx: "react",
  tsx: "react",
};

export function parseKind(s: string | undefined): ArtifactKind | null {
  if (!s) return null;
  const k = s.trim().toLowerCase().split("/").pop() ?? "";
  return KINDS[k] ?? (k === "svg+xml" ? "svg" : null);
}

/** What an `artifact` tool call says, as far as it has arrived. */
export interface ArtifactCallInfo {
  command: "create" | "update" | "rewrite" | null;
  key: string;
  kind: ArtifactKind | null;
  title: string;
  language: string | null;
  /** create/rewrite: the content so far. */
  content: string | null;
}

export function artifactCallInfo(inputJson: string): ArtifactCallInfo {
  const f = partialFields(inputJson);
  const command =
    f.command === "create" || f.command === "update" || f.command === "rewrite" ? f.command : null;
  return {
    command,
    key: f.id ?? "",
    kind: parseKind(f.type),
    title: (f.title ?? "").replace(/\s+/g, " ").trim(),
    language: f.language?.trim() || null,
    content: f.content ?? null,
  };
}

/** The artifact a tool result points to (`artifactRef` block). */
export function artifactRefOf(
  content: ContentBlock[] | null | undefined,
): { artifactId: string; title: string; version: number } | null {
  for (const b of content ?? []) {
    if (b.type === "artifactRef") {
      return { artifactId: b.artifactId, title: b.title, version: b.version };
    }
  }
  return null;
}
