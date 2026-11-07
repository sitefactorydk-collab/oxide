import {
  AppWindow,
  Component,
  FileCode,
  FileText,
  Image as ImageIcon,
  Shapes,
  Workflow,
  type LucideIcon,
} from "lucide-react";
import type { ArtifactKind } from "@/bindings";

const ICONS: Record<ArtifactKind, LucideIcon> = {
  markdown: FileText,
  code: FileCode,
  html: AppWindow,
  svg: ImageIcon,
  mermaid: Workflow,
  react: Component,
};

export function KindIcon({
  kind,
  className = "size-4",
}: {
  kind: ArtifactKind | null;
  className?: string;
}) {
  const Icon = kind ? ICONS[kind] : Shapes;
  return <Icon aria-hidden className={className} />;
}
