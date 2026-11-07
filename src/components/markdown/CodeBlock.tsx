import { Check, Copy, WrapText } from "lucide-react";
import { memo, useEffect, useState, type CSSProperties } from "react";
import { IconButton } from "@/components/ui/IconButton";
import { useCopy } from "@/lib/useCopy";
import { cn } from "@/lib/cn";
import { cachedTokens, highlight, type Tokens } from "./highlight";

/** Wait this long after the code last changed before highlighting (it may still stream). */
const HIGHLIGHT_DELAY_MS = 120;

/** Shiki font-style bit flags. */
const ITALIC = 1;
const BOLD = 2;
const UNDERLINE = 4;

function tokenStyle(color: string | undefined, fontStyle: number | undefined): CSSProperties {
  const s: CSSProperties = {};
  if (color) s.color = color;
  if (fontStyle && fontStyle > 0) {
    if (fontStyle & ITALIC) s.fontStyle = "italic";
    if (fontStyle & BOLD) s.fontWeight = 600;
    if (fontStyle & UNDERLINE) s.textDecoration = "underline";
  }
  return s;
}

/**
 * Highlighted tokens for `code`, or null while plain text is shown. Plain and highlighted
 * output have exactly the same text, so colours appear without moving anything. While code
 * streams in, the last result stays on screen for the lines it covers.
 */
function useTokens(lang: string, code: string): Tokens | null {
  const cached = cachedTokens(lang, code);
  const [last, setLast] = useState<Tokens | null>(cached ?? null);

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

function Highlighted({ tokens, code }: { tokens: Tokens; code: string }) {
  // While streaming, tokens may cover an older, shorter version of the code: render the
  // highlighted prefix, then the rest as plain text.
  const highlightedText = tokens.map((line) => line.map((t) => t.content).join("")).join("\n");
  const prefixOk = code.startsWith(highlightedText);
  if (!prefixOk) return <>{code}</>;
  const rest = code.slice(highlightedText.length);
  return (
    <>
      {tokens.map((line, i) => (
        <span key={i}>
          {line.map((t, j) => (
            <span key={j} style={tokenStyle(t.color, t.fontStyle)}>
              {t.content}
            </span>
          ))}
          {i < tokens.length - 1 ? "\n" : null}
        </span>
      ))}
      {rest}
    </>
  );
}

/** A fenced code block: language label, copy and wrap buttons, highlighted code. */
export const CodeBlock = memo(function CodeBlock({ lang, code }: { lang: string; code: string }) {
  const [wrap, setWrap] = useState(false);
  const [copied, copy] = useCopy();
  const tokens = useTokens(lang, code);
  const label = lang.trim() || "text";

  return (
    <div className="ox-code group/code overflow-hidden rounded-lg border border-line bg-code font-sans">
      <div className="flex h-8 items-center gap-1 border-b border-line pl-3 pr-1">
        <span className="flex-1 truncate font-mono text-xs text-fg-subtle">{label}</span>
        <IconButton
          size="sm"
          label={wrap ? "Don’t wrap lines" : "Wrap lines"}
          aria-pressed={wrap}
          active={wrap}
          onClick={() => setWrap((w) => !w)}
        >
          <WrapText className="size-3.5" />
        </IconButton>
        <IconButton size="sm" label={copied ? "Copied" : "Copy code"} onClick={() => copy(code)}>
          {copied ? <Check className="size-3.5" /> : <Copy className="size-3.5" />}
        </IconButton>
      </div>
      <pre
        tabIndex={0}
        aria-label={`${label} code`}
        className={cn(
          "!m-0 !rounded-none !border-0 px-4 py-3",
          wrap ? "whitespace-pre-wrap break-words" : "overflow-x-auto whitespace-pre",
        )}
      >
        <code>{tokens ? <Highlighted tokens={tokens} code={code} /> : code}</code>
      </pre>
    </div>
  );
});

/**
 * Highlighted source filling its container, without the block's header (the artifact
 * panel's code view).
 */
export const CodeView = memo(function CodeView({ lang, code }: { lang: string; code: string }) {
  const tokens = useTokens(lang, code);
  return (
    <pre
      tabIndex={0}
      aria-label={`${lang || "text"} source`}
      className="ox-code !m-0 min-h-full overflow-x-auto whitespace-pre !rounded-none !border-0 bg-code px-4 py-3 font-mono text-sm leading-6"
    >
      <code>{tokens ? <Highlighted tokens={tokens} code={code} /> : code}</code>
    </pre>
  );
});
