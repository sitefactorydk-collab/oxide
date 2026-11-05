/**
 * Prepares model output for react-markdown + remark-math.
 *
 * - LaTeX delimiters `\(…\)` and `\[…\]` (common in model output) become `$…$` / `$$…$$`.
 * - Single dollars follow Pandoc's rule, so prices stay text: an opening `$` must be followed
 *   by a non-space, and the closing `$` preceded by a non-space and not followed by a digit.
 *   Any other `$` is escaped. "$5 and $10" stays text; "$x^2$" is math.
 * - Code (fenced blocks and inline spans) is never touched.
 * - While streaming, unfinished constructs are completed or held back so the layout doesn't
 *   jump: an open code fence is closed, an unclosed `$$` block or `\[` is held back until it
 *   closes, and a half-written table row waits for its line to end.
 */

export interface PrepareOptions {
  /** The text is still arriving. */
  streaming?: boolean;
}

type Segment = { code: boolean; text: string };

const FENCE_OPEN = /^(\s*)(`{3,}|~{3,})(.*)$/;

/** Splits text into fenced code blocks and everything else. */
function splitFences(text: string, streaming: boolean): Segment[] {
  const lines = text.split("\n");
  const out: Segment[] = [];
  let buf: string[] = [];
  let fence: { char: string; len: number; indent: string } | null = null;

  const flush = (code: boolean) => {
    if (buf.length) out.push({ code, text: buf.join("\n") });
    buf = [];
  };

  lines.forEach((line) => {
    if (!fence) {
      const m = FENCE_OPEN.exec(line);
      // A backtick fence's info string can't contain backticks.
      if (m && !(m[2]!.startsWith("`") && m[3]!.includes("`"))) {
        flush(false);
        fence = { char: m[2]![0]!, len: m[2]!.length, indent: m[1]! };
        buf.push(line);
        return;
      }
      buf.push(line);
      return;
    }
    buf.push(line);
    const close = /^\s*(`{3,}|~{3,})\s*$/.exec(line);
    if (close && close[1]![0] === fence.char && close[1]!.length >= fence.len) {
      fence = null;
      flush(true);
    }
  });

  if (fence) {
    const f = fence as { char: string; len: number; indent: string };
    if (streaming) buf.push(f.indent + f.char.repeat(f.len));
    flush(true);
  } else {
    flush(false);
  }
  // Rejoin with the newlines between segments.
  return out.map((s, i) => (i < out.length - 1 ? { ...s, text: `${s.text}\n` } : s));
}

const isSpace = (c: string | undefined) => c === undefined || /\s/.test(c);

/** Index of the next unescaped `needle` at or after `from`, or -1. */
function findUnescaped(text: string, needle: string, from: number, stopAtBlank = false): number {
  for (let j = from; j < text.length; j++) {
    const c = text[j];
    if (c === "\\") {
      j++;
      continue;
    }
    if (stopAtBlank && c === "\n" && text[j + 1] === "\n") return -1;
    if (text.startsWith(needle, j)) return j;
  }
  return -1;
}

/**
 * Indentation that keeps a block inside the line's container: its leading whitespace, plus
 * the width of a list marker ("- ", "1. ") if the line starts a list item.
 */
function lineIndent(text: string, i: number): string {
  const start = text.lastIndexOf("\n", i - 1) + 1;
  const line = text.slice(start);
  const item = /^([ \t]*)([-*+]|\d{1,9}[.)])[ \t]+/.exec(line);
  if (item) return " ".repeat(item[0].length);
  return /^[ \t]*/.exec(line)![0];
}

function atLineStart(text: string, i: number): boolean {
  const start = text.lastIndexOf("\n", i - 1) + 1;
  return text.slice(start, i).trim() === "";
}

/** Display math as its own block, indented like the line it was on (so lists keep it). */
function displayMath(text: string, at: number, inner: string): string {
  const indent = lineIndent(text, at);
  const body = inner
    .trim()
    .split("\n")
    .map((l) => indent + l.trim())
    .join("\n");
  const lead = atLineStart(text, at) ? "" : `\n${indent}`;
  return `${lead}$$\n${body}\n${indent}$$\n`;
}

/**
 * Math delimiters and dollars in non-code text. Returns the text and whether the rest was
 * held back (streaming only).
 */
function processText(text: string, holdBack: boolean): string {
  let out = "";
  let i = 0;
  while (i < text.length) {
    const c = text[i]!;

    if (c === "\\") {
      const next = text[i + 1];
      if (next === "(" || next === "[") {
        const closer = next === "(" ? "\\)" : "\\]";
        const end = text.indexOf(closer, i + 2);
        if (end >= 0) {
          const inner = text.slice(i + 2, end);
          if (next === "(") {
            out += `$${inner.trim()}$`;
            i = end + 2;
            continue;
          }
          out += displayMath(text, i, inner);
          const indent = lineIndent(text, i);
          i = end + 2;
          // Text after the block on the same line continues in the same container.
          while (text[i] === " " || text[i] === "\t") i++;
          if (i < text.length && text[i] !== "\n") out += indent;
          else if (text[i] === "\n") i++;
          continue;
        }
        if (holdBack && next === "[") return out;
      }
      out += c + (next ?? "");
      i += 2;
      continue;
    }

    if (c === "`") {
      let n = 1;
      while (text[i + n] === "`") n++;
      const run = "`".repeat(n);
      // The closing run must be exactly n backticks.
      let j = text.indexOf(run, i + n);
      while (j >= 0 && (text[j + n] === "`" || text[j - 1] === "`")) {
        j = text.indexOf(run, j + 1);
      }
      if (j >= 0) {
        out += text.slice(i, j + n);
        i = j + n;
        continue;
      }
      // An inline code span still being written: wait for it (but not across paragraphs).
      if (holdBack && !text.slice(i).includes("\n\n")) return out;
      out += run;
      i += n;
      continue;
    }

    if (c === "$") {
      if (text[i + 1] === "$") {
        const end = findUnescaped(text, "$$", i + 2);
        if (end >= 0) {
          out += text.slice(i, end + 2);
          i = end + 2;
          continue;
        }
        if (holdBack) return out;
        out += "\\$\\$";
        i += 2;
        continue;
      }
      if (!isSpace(text[i + 1])) {
        const j = findUnescaped(text, "$", i + 1, true);
        if (
          j > i + 1 &&
          !isSpace(text[j - 1]) &&
          !/\d/.test(text[j + 1] ?? "") &&
          text[j + 1] !== "$"
        ) {
          out += text.slice(i, j + 1);
          i = j + 1;
          continue;
        }
      }
      out += "\\$";
      i += 1;
      continue;
    }

    out += c;
    i += 1;
  }
  return out;
}

export function prepareMarkdown(src: string, { streaming = false }: PrepareOptions = {}): string {
  const text = src.replace(/\r\n?/g, "\n");
  const segments = splitFences(text, streaming);
  let out = "";
  for (let k = 0; k < segments.length; k++) {
    const seg = segments[k]!;
    const last = k === segments.length - 1;
    if (seg.code) {
      out += seg.text;
      continue;
    }
    let t = processText(seg.text, streaming && last);
    if (streaming && last && !t.endsWith("\n")) {
      // A table row (or delimiter row) whose line hasn't ended yet.
      const lineStart = t.lastIndexOf("\n") + 1;
      if (/^\s*\|/.test(t.slice(lineStart))) t = t.slice(0, lineStart);
    }
    out += t;
  }
  return out;
}
