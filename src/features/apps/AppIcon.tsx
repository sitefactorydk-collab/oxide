import { Sparkles } from "lucide-react";
import { cn } from "@/lib/cn";
import { APP_ICONS } from "./appIcons";

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
