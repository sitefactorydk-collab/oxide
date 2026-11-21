import { ChevronDown, Hand, Zap } from "lucide-react";
import type { PermissionMode } from "@/bindings";
import {
  MenuContent,
  MenuItem,
  MenuLabel,
  MenuRadioGroup,
  MenuRadioItem,
  MenuRoot,
  MenuSeparator,
  MenuTrigger,
} from "@/components/ui/Menu";
import { Tooltip } from "@/components/ui/Tooltip";
import { useOpenSettings } from "@/features/settings/useOpenSettings";
import { cn } from "@/lib/cn";
import { MODE_HINTS, MODE_LABELS } from "./labels";
import { usePermissionMode } from "./queries";

function ModeIcon({ mode, className }: { mode: PermissionMode; className?: string }) {
  const Icon = mode === "auto" ? Zap : Hand;
  return <Icon aria-hidden className={className} />;
}

/** The composer's **Permission mode** menu (Manual / Auto), kept per chat (Phase 23). */
export function PermissionModeMenu({ conversationId }: { conversationId?: string }) {
  const { mode, setMode } = usePermissionMode(conversationId);
  const openSettings = useOpenSettings();
  const title = `Permission mode: ${MODE_LABELS[mode]}`;
  return (
    <MenuRoot>
      <Tooltip label={MODE_HINTS[mode]}>
        <MenuTrigger asChild>
          <button
            type="button"
            aria-label={title}
            className={cn(
              "inline-flex h-8 items-center gap-1 rounded-lg px-2 text-sm transition-colors data-[state=open]:bg-hover",
              mode === "auto"
                ? "bg-accent-soft text-accent hover:bg-accent-soft"
                : "text-fg-muted hover:bg-hover hover:text-fg",
            )}
          >
            <ModeIcon mode={mode} className="size-[17px]" />
            <span className={mode === "auto" ? "font-medium" : undefined}>{MODE_LABELS[mode]}</span>
            <ChevronDown aria-hidden className="size-3.5" />
          </button>
        </MenuTrigger>
      </Tooltip>
      <MenuContent align="end" side="top" className="w-80">
        <MenuLabel>Permission mode</MenuLabel>
        <MenuRadioGroup value={mode} onValueChange={(v) => setMode(v as PermissionMode)}>
          {(["manual", "auto"] as const).map((m) => (
            <MenuRadioItem key={m} value={m}>
              <span className="flex min-w-0 flex-1 items-start gap-2">
                <ModeIcon mode={m} className="mt-0.5 size-4 shrink-0 text-fg-muted" />
                <span className="flex min-w-0 flex-col">
                  <span>{MODE_LABELS[m]}</span>
                  <span className="text-xs text-fg-subtle">{MODE_HINTS[m]}</span>
                </span>
              </span>
            </MenuRadioItem>
          ))}
        </MenuRadioGroup>
        <MenuSeparator />
        <MenuItem onSelect={() => openSettings("permissions")}>Permission settings…</MenuItem>
      </MenuContent>
    </MenuRoot>
  );
}
