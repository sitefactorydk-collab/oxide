import {
  BookOpen,
  Bot,
  Braces,
  Brush,
  Calculator,
  ChartColumn,
  Code,
  FileCode,
  FileText,
  Film,
  Globe,
  Image,
  Languages,
  LayoutGrid,
  ListChecks,
  Mail,
  MessageSquare,
  Music,
  PenTool,
  Puzzle,
  Search,
  Sparkles,
  Table,
  Tags,
  TriangleAlert,
  Wand,
  type LucideIcon,
} from "lucide-react";
import { cn } from "@/lib/cn";

/** The lucide icons an app's manifest may name (kebab-case). Others show as sparkles. */
const APP_ICONS: Record<string, LucideIcon> = {
  "book-open": BookOpen,
  bot: Bot,
  braces: Braces,
  brush: Brush,
  calculator: Calculator,
  "chart-column": ChartColumn,
  code: Code,
  "file-code": FileCode,
  "file-text": FileText,
  film: Film,
  globe: Globe,
  image: Image,
  languages: Languages,
  "layout-grid": LayoutGrid,
  "list-checks": ListChecks,
  mail: Mail,
  "message-square": MessageSquare,
  music: Music,
  "pen-tool": PenTool,
  puzzle: Puzzle,
  search: Search,
  sparkles: Sparkles,
  table: Table,
  tags: Tags,
  "triangle-alert": TriangleAlert,
  wand: Wand,
};

/** An app's icon: a lucide icon by name, or an emoji. */
export function AppIcon({ icon, className }: { icon: string; className?: string }) {
  const Icon = APP_ICONS[icon];
  const box = cn(
    "flex shrink-0 items-center justify-center rounded-xl bg-accent-soft text-accent",
    className ?? "size-10",
  );
  if (Icon) {
    return (
      <span className={box} aria-hidden>
        <Icon className="size-1/2" />
      </span>
    );
  }
  const emoji = /^[^\w\s]{1,8}$/u.test(icon.trim());
  return (
    <span className={cn(box, "text-xl")} aria-hidden>
      {emoji ? icon.trim() : <Sparkles className="size-1/2" />}
    </span>
  );
}
