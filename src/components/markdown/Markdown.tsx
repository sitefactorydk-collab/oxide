import { ExternalLink, Image as ImageIcon } from "lucide-react";
import { memo, useDeferredValue, useMemo, type MouseEvent, type ReactNode } from "react";
import ReactMarkdown, { type Components } from "react-markdown";
import rehypeKatex from "rehype-katex";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import { isExternalUrl, openExternal } from "@/lib/links";
import { CodeBlock } from "./CodeBlock";
import { prepareMarkdown } from "./prepare";

/** The parts of a hast node the renderers read. */
interface HastNode {
  type: string;
  value?: string;
  tagName?: string;
  properties?: Record<string, unknown>;
  children?: HastNode[];
}

function textOf(n: HastNode): string {
  if (n.type === "text") return n.value ?? "";
  return (n.children ?? []).map(textOf).join("");
}

function languageOf(n: HastNode): string {
  const cls = n.properties?.className;
  const list = Array.isArray(cls) ? cls.map(String) : typeof cls === "string" ? cls.split(" ") : [];
  const lang = list.find((c) => c.startsWith("language-"));
  return lang ? lang.slice("language-".length) : "";
}

function onLinkClick(href: string | undefined) {
  return (e: MouseEvent) => {
    // Links never navigate the app window: web and mail links open outside, in-page links
    // (footnotes) scroll.
    e.preventDefault();
    if (!href) return;
    if (href.startsWith("#")) {
      document.getElementById(decodeURIComponent(href.slice(1)))?.scrollIntoView({
        block: "center",
      });
    } else if (isExternalUrl(href)) {
      openExternal(href);
    }
  };
}

const components: Components = {
  pre({ node, children }) {
    const code = (node as HastNode | undefined)?.children?.[0];
    if (!code || code.tagName !== "code") return <pre>{children}</pre>;
    let text = textOf(code);
    if (text.endsWith("\n")) text = text.slice(0, -1);
    return <CodeBlock lang={languageOf(code)} code={text} />;
  },
  a({ href, children }) {
    return (
      <a href={href} title={href} onClick={onLinkClick(href)}>
        {children}
      </a>
    );
  },
  img({ src, alt }) {
    // Remote images aren't loaded (privacy, and the CSP blocks them): show a link instead.
    const url = typeof src === "string" ? src : undefined;
    return (
      <a href={url} title={url} onClick={onLinkClick(url)} className="ox-image-link">
        <ImageIcon aria-hidden className="inline size-[0.95em] align-[-0.1em]" /> {alt || "Image"}
        {isExternalUrl(url) && (
          <ExternalLink aria-hidden className="ml-0.5 inline size-[0.8em] align-[-0.05em]" />
        )}
      </a>
    );
  },
  table({ children }) {
    return (
      <div className="ox-table" tabIndex={0} role="region" aria-label="Table">
        <table>{children}</table>
      </div>
    );
  },
};

const remarkPlugins = [remarkGfm, remarkMath];
const rehypePlugins: Parameters<typeof ReactMarkdown>[0]["rehypePlugins"] = [
  [rehypeKatex, { throwOnError: false, strict: "ignore", errorColor: "var(--ox-danger)" }],
];

const Rendered = memo(function Rendered({ source }: { source: string }) {
  return (
    <ReactMarkdown
      remarkPlugins={remarkPlugins}
      rehypePlugins={rehypePlugins}
      components={components}
    >
      {source}
    </ReactMarkdown>
  );
});

/**
 * Assistant Markdown: GFM (tables, task lists, strikethrough, footnotes), KaTeX math and
 * Shiki-highlighted code blocks. With `streaming`, partial input is completed or held back
 * (see `prepareMarkdown`) and re-renders are deferred under load, so a reply arriving 25
 * times a second doesn't flicker or jump.
 */
export const Markdown = memo(function Markdown({
  text,
  streaming = false,
  className = "ox-prose",
}: {
  text: string;
  streaming?: boolean;
  className?: string;
}): ReactNode {
  const deferred = useDeferredValue(text);
  const input = streaming ? deferred : text;
  const source = useMemo(() => prepareMarkdown(input, { streaming }), [input, streaming]);
  return (
    <div className={className}>
      <Rendered source={source} />
    </div>
  );
});
