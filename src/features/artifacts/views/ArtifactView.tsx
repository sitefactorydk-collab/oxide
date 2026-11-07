import type { ArtifactKind } from "@/bindings";
import { CodeView } from "@/components/markdown/CodeBlock";
import { Markdown } from "@/components/markdown/Markdown";
import { sourceLanguage, svgDataUrl } from "../kinds";
import type { PanelView } from "../store";
import { FrameView } from "./FrameView";
import { MermaidView } from "./MermaidView";

/** The body of the artifact panel: the rendered artifact, or its source. */
export function ArtifactView({
  kind,
  language,
  content,
  title,
  view,
  streaming = false,
}: {
  kind: ArtifactKind;
  language: string | null;
  content: string;
  title: string;
  view: PanelView;
  /** The content is still arriving: show it as source. */
  streaming?: boolean;
}) {
  if (view === "code" || kind === "code" || (streaming && kind !== "markdown")) {
    return <CodeView lang={sourceLanguage(kind, language)} code={content} />;
  }
  switch (kind) {
    case "markdown":
      return (
        <div className="mx-auto max-w-thread px-8 py-6">
          <Markdown text={content} streaming={streaming} />
        </div>
      );
    case "svg":
      return (
        <div className="flex min-h-full items-center justify-center p-6">
          <img
            alt={title}
            src={svgDataUrl(content)}
            className="max-h-full max-w-full rounded-md bg-white object-contain"
          />
        </div>
      );
    case "mermaid":
      return <MermaidView content={content} />;
    case "html":
    case "react":
      // A fresh frame for every content.
      return <FrameView key={`${kind}:${content}`} kind={kind} content={content} title={title} />;
  }
}
