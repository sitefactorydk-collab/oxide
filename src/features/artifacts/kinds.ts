import type { ArtifactKind } from "@/bindings";

export const KIND_LABELS: Record<ArtifactKind, string> = {
  markdown: "Document",
  code: "Code",
  html: "Web page",
  svg: "Image",
  mermaid: "Diagram",
  react: "Interactive app",
};

/** "Code · Python", "Web page", … */
export function kindLine(kind: ArtifactKind | null, language: string | null): string {
  if (!kind) return "Artifact";
  if (kind === "code" && language) return `Code · ${prettyLanguage(language)}`;
  return KIND_LABELS[kind];
}

function prettyLanguage(l: string): string {
  const known: Record<string, string> = {
    js: "JavaScript",
    javascript: "JavaScript",
    ts: "TypeScript",
    typescript: "TypeScript",
    py: "Python",
    python: "Python",
    rs: "Rust",
    rust: "Rust",
    cs: "C#",
    csharp: "C#",
    cpp: "C++",
    sql: "SQL",
    css: "CSS",
    html: "HTML",
    json: "JSON",
    yaml: "YAML",
    sh: "Shell",
    bash: "Bash",
    powershell: "PowerShell",
    go: "Go",
  };
  return known[l.toLowerCase()] ?? l.charAt(0).toUpperCase() + l.slice(1);
}

/** Which kinds have a rendered preview (code is shown as code only). */
export function hasPreview(kind: ArtifactKind): boolean {
  return kind !== "code";
}

/** The language used to highlight a kind's source. */
export function sourceLanguage(kind: ArtifactKind, language: string | null): string {
  switch (kind) {
    case "markdown":
      return "markdown";
    case "html":
      return "html";
    case "svg":
      return "xml";
    case "mermaid":
      return "text";
    case "react":
      return "tsx";
    case "code":
      return language ?? "text";
  }
}

/** An SVG as an image URL: scripts and event handlers in it never run. */
export function svgDataUrl(svg: string): string {
  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
}
