import { AlertTriangle, Loader2 } from "lucide-react";
import { useEffect, useId, useState } from "react";
import { useResolvedTheme } from "@/app/useThemeSync";

let counter = 0;

/** Renders Mermaid source to SVG (Mermaid is bundled and loaded on first use). */
export function MermaidView({ content }: { content: string }) {
  const dark = useResolvedTheme() === "dark";
  const base = useId().replace(/[^a-zA-Z0-9]/g, "");
  const [svg, setSvg] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    setError(null);
    void (async () => {
      try {
        const mermaid = (await import("mermaid")).default;
        mermaid.initialize({
          startOnLoad: false,
          // Strict: labels are sanitized and click handlers / scripts are dropped.
          securityLevel: "strict",
          theme: dark ? "dark" : "default",
          fontFamily: "Inter Variable, Inter, system-ui, sans-serif",
        });
        counter += 1;
        const { svg } = await mermaid.render(`m${base}${counter}`, content);
        if (alive) setSvg(svg);
      } catch (e) {
        if (alive) setError(e instanceof Error ? e.message : String(e));
      }
    })();
    return () => {
      alive = false;
    };
  }, [content, dark, base]);

  if (error) {
    return (
      <div
        role="alert"
        className="m-4 flex items-start gap-2 rounded-lg border border-line bg-raised p-3 text-sm"
      >
        <AlertTriangle aria-hidden className="mt-0.5 size-4 shrink-0 text-danger" />
        <div className="min-w-0">
          <p className="text-fg">The diagram couldn’t be drawn.</p>
          <pre className="mt-1 whitespace-pre-wrap break-words font-mono text-xs text-fg-muted">
            {error}
          </pre>
        </div>
      </div>
    );
  }
  if (svg === null) {
    return (
      <div className="flex h-full items-center justify-center">
        <Loader2 aria-label="Drawing diagram" className="size-5 animate-spin text-fg-subtle" />
      </div>
    );
  }
  return (
    <div
      aria-label="Diagram"
      role="img"
      className="flex min-h-full items-start justify-center p-6 [&_svg]:h-auto [&_svg]:max-w-full"
      // Mermaid's own output in strict mode (sanitized with DOMPurify).
      dangerouslySetInnerHTML={{ __html: svg }}
    />
  );
}
