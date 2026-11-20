import { ChevronDown, ChevronUp, X } from "lucide-react";
import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type KeyboardEvent,
  type ReactNode,
} from "react";
import { cachedTokens, highlight, type Tokens } from "@/components/markdown/highlight";
import { IconButton } from "@/components/ui/IconButton";
import { cn } from "@/lib/cn";
import { findAll, languageOf } from "./editorText";

/** Line height of the editor in px (the textarea and the highlighted copy must match). */
const LINE = 20;
const PAD = 12;
const HIGHLIGHT_DELAY_MS = 150;

/** Tokens for `code`; while typing, the last ones (lines that changed show plain). */
function useTokens(lang: string, code: string): Tokens | null {
  const cached = cachedTokens(lang, code);
  const [last, setLast] = useState<Tokens | null>(null);
  useEffect(() => {
    if (cached) return;
    let alive = true;
    const t = setTimeout(() => {
      void highlight(lang, code).then((tokens) => {
        if (alive && tokens) setLast(tokens);
      });
    }, HIGHLIGHT_DELAY_MS);
    return () => {
      alive = false;
      clearTimeout(t);
    };
  }, [lang, code, cached]);
  return cached ?? last;
}

function tokenStyle(color: string | undefined, fontStyle: number | undefined): CSSProperties {
  const s: CSSProperties = {};
  if (color) s.color = color;
  // Italic keeps the monospace advance; bold may not, so it isn't used here.
  if (fontStyle && fontStyle & 1) s.fontStyle = "italic";
  return s;
}

const layer =
  "pointer-events-none absolute left-0 top-0 m-0 whitespace-pre font-mono text-[13px] leading-[20px]";

/**
 * A code editor: a textarea over a syntax-highlighted copy of its text, with line numbers,
 * Tab indenting, Ctrl+S to save and Ctrl+F to find.
 */
export function CodeEditor({
  path,
  value,
  onChange,
  onSave,
  readOnly = false,
}: {
  path: string;
  value: string;
  onChange: (value: string) => void;
  onSave?: () => void;
  readOnly?: boolean;
}) {
  const area = useRef<HTMLTextAreaElement>(null);
  const findInput = useRef<HTMLInputElement>(null);
  const [scroll, setScroll] = useState({ top: 0, left: 0 });
  const [finding, setFinding] = useState(false);
  const [query, setQuery] = useState("");
  const [current, setCurrent] = useState(0);
  const tokens = useTokens(languageOf(path), value);
  const textLines = useMemo(() => value.split("\n"), [value]);
  const lines = textLines.length;
  const matches = useMemo(() => (finding ? findAll(value, query) : []), [finding, value, query]);
  const at = matches.length ? Math.min(current, matches.length - 1) : -1;

  useEffect(() => {
    if (finding) findInput.current?.select();
  }, [finding]);

  // Scroll the current match into view.
  useEffect(() => {
    const el = area.current;
    const start = matches[at];
    if (!el || start === undefined) return;
    const line = value.slice(0, start).split("\n").length - 1;
    const y = PAD + line * LINE;
    if (y < el.scrollTop || y > el.scrollTop + el.clientHeight - LINE * 2) {
      el.scrollTop = Math.max(0, y - el.clientHeight / 3);
    }
  }, [at, matches, value]);

  const step = (d: number) => {
    if (matches.length === 0) return;
    setCurrent((c) => (Math.min(c, matches.length - 1) + d + matches.length) % matches.length);
  };

  const onKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    const mod = e.ctrlKey || e.metaKey;
    if (mod && e.key.toLowerCase() === "s") {
      e.preventDefault();
      onSave?.();
      return;
    }
    if (mod && e.key.toLowerCase() === "f") {
      e.preventDefault();
      const el = e.currentTarget;
      const selected = el.value.slice(el.selectionStart, el.selectionEnd);
      if (selected && !selected.includes("\n")) setQuery(selected);
      setFinding(true);
      findInput.current?.select();
      return;
    }
    if (e.key === "Escape" && finding) {
      setFinding(false);
      return;
    }
    if (e.key === "Tab" && !readOnly && !mod && !e.altKey) {
      e.preventDefault();
      const el = e.currentTarget;
      const { selectionStart: s, selectionEnd: end } = el;
      if (e.shiftKey) {
        // Outdent the current line.
        const lineStart = el.value.lastIndexOf("\n", s - 1) + 1;
        const n = el.value.slice(lineStart, lineStart + 2) === "  " ? 2 : 0;
        if (n === 0) return;
        onChange(el.value.slice(0, lineStart) + el.value.slice(lineStart + n));
        requestAnimationFrame(() => el.setSelectionRange(Math.max(lineStart, s - n), end - n));
        return;
      }
      onChange(`${el.value.slice(0, s)}  ${el.value.slice(end)}`);
      requestAnimationFrame(() => el.setSelectionRange(s + 2, s + 2));
    }
  };

  const shift: CSSProperties = {
    transform: `translate(${-scroll.left}px, ${-scroll.top}px)`,
    padding: PAD,
  };

  // The match layer: transparent text with the matches marked.
  const marks = useMemo(() => {
    if (matches.length === 0) return null;
    const parts: ReactNode[] = [];
    let last = 0;
    matches.forEach((start, i) => {
      parts.push(value.slice(last, start));
      parts.push(
        <mark
          key={start}
          className={cn("rounded-sm text-transparent", i === at ? "bg-accent/50" : "bg-accent/20")}
        >
          {value.slice(start, start + query.length)}
        </mark>,
      );
      last = start + query.length;
    });
    parts.push(value.slice(last));
    return parts;
  }, [matches, at, value, query]);

  return (
    <div className="relative flex min-h-0 flex-1 flex-col">
      {finding && (
        <div
          role="search"
          aria-label="Find in file"
          className="absolute right-4 top-2 z-10 flex items-center gap-1 rounded-lg border border-line bg-raised p-1 shadow-md"
        >
          <input
            ref={findInput}
            value={query}
            aria-label="Find"
            placeholder="Find"
            onChange={(e) => {
              setQuery(e.target.value);
              setCurrent(0);
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                step(e.shiftKey ? -1 : 1);
              } else if (e.key === "Escape") {
                setFinding(false);
                area.current?.focus();
              }
            }}
            className="h-7 w-44 rounded-md bg-transparent px-2 text-sm text-fg outline-none"
          />
          <span className="w-16 text-center text-xs text-fg-subtle" aria-live="polite">
            {query ? (matches.length ? `${at + 1} of ${matches.length}` : "No results") : ""}
          </span>
          <IconButton size="sm" label="Previous match" tooltip={false} onClick={() => step(-1)}>
            <ChevronUp className="size-4" />
          </IconButton>
          <IconButton size="sm" label="Next match" tooltip={false} onClick={() => step(1)}>
            <ChevronDown className="size-4" />
          </IconButton>
          <IconButton
            size="sm"
            label="Close find"
            tooltip={false}
            onClick={() => {
              setFinding(false);
              area.current?.focus();
            }}
          >
            <X className="size-4" />
          </IconButton>
        </div>
      )}
      <div className="flex min-h-0 flex-1 overflow-hidden bg-sunken">
        <div
          aria-hidden
          className="relative w-12 shrink-0 overflow-hidden border-r border-line text-right font-mono text-[12px] leading-[20px] text-fg-subtle select-none"
        >
          <div style={{ transform: `translateY(${-scroll.top}px)`, padding: `${PAD}px 8px` }}>
            {Array.from({ length: lines }, (_, i) => (
              <div key={i}>{i + 1}</div>
            ))}
          </div>
        </div>
        <div className="relative min-w-0 flex-1 overflow-hidden">
          <pre aria-hidden className={cn(layer, "text-fg")} style={shift}>
            {textLines.map((text, i) => {
              const line = tokens?.[i];
              const colored = line && line.map((t) => t.content).join("") === text;
              return (
                <span key={i}>
                  {colored
                    ? line.map((t, j) => (
                        <span key={j} style={tokenStyle(t.color, t.fontStyle)}>
                          {t.content}
                        </span>
                      ))
                    : text}
                  {i < textLines.length - 1 ? "\n" : null}
                </span>
              );
            })}
            {"\n"}
          </pre>
          {marks && (
            <pre aria-hidden className={cn(layer, "text-transparent")} style={shift}>
              {marks}
            </pre>
          )}
          <textarea
            ref={area}
            aria-label={path}
            value={value}
            readOnly={readOnly}
            spellCheck={false}
            autoCapitalize="off"
            autoCorrect="off"
            wrap="off"
            onChange={(e) => onChange(e.target.value)}
            onKeyDown={onKeyDown}
            onScroll={(e) =>
              setScroll({ top: e.currentTarget.scrollTop, left: e.currentTarget.scrollLeft })
            }
            className="absolute inset-0 h-full w-full resize-none overflow-auto whitespace-pre bg-transparent font-mono text-[13px] leading-[20px] text-transparent caret-fg outline-none selection:bg-accent/25"
            style={{ padding: PAD, tabSize: 2 }}
          />
        </div>
      </div>
    </div>
  );
}
