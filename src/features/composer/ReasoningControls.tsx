import { ChevronDown, Gauge, Lightbulb } from "lucide-react";
import type { Effort, ModelInfo } from "@/bindings";
import {
  MenuContent,
  MenuLabel,
  MenuRadioGroup,
  MenuRadioItem,
  MenuRoot,
  MenuSeparator,
  MenuTrigger,
} from "@/components/ui/Menu";
import { Tooltip } from "@/components/ui/Tooltip";
import { EFFORT_LABELS, alwaysThinks, effectiveEffort } from "@/features/models/models";
import { cn } from "@/lib/cn";
import { useUi } from "@/stores/ui";

const EFFORT_HINTS: Record<Effort, string> = {
  low: "Fastest and cheapest; fine for simple questions",
  medium: "Balanced",
  high: "Thorough answers to hard problems",
  xhigh: "Extended effort for long, demanding work",
  max: "No limit on effort; slowest",
};

/**
 * Extended-thinking switch. Unknown (unlisted) models may think, so the toggle stays
 * available and the provider decides. Models that always think show it on and locked.
 */
export function ThinkingToggle({ model }: { model: ModelInfo | undefined }) {
  const thinking = useUi((s) => s.thinking);
  const toggle = useUi((s) => s.toggleThinking);
  const caps = model?.capabilities;
  const always = !!caps && alwaysThinks(caps);
  const supported = !caps || (caps.thinking !== "none" && !always);
  const on = always || (thinking && supported);
  const label = always
    ? caps?.effort.length
      ? "This model always thinks. Use Effort to set how much."
      : "This model always thinks before answering"
    : !supported
      ? "This model can’t think before answering"
      : caps?.thinking === "switch"
        ? on
          ? "Thinking on (the server decides how long)"
          : "Thinking off"
        : on
          ? "Extended thinking on"
          : "Extended thinking off";
  return (
    <Tooltip label={label}>
      {/* A span keeps the tooltip working while the button is disabled. */}
      <span className="inline-flex">
        <button
          type="button"
          aria-pressed={on}
          aria-label="Extended thinking"
          disabled={!supported}
          onClick={toggle}
          className={cn(
            "inline-flex h-8 items-center gap-1.5 rounded-lg px-2 text-sm transition-colors disabled:opacity-40",
            on ? "bg-accent-soft text-accent" : "text-fg-muted hover:bg-hover hover:text-fg",
            always && "disabled:opacity-70",
          )}
        >
          <Lightbulb className="size-[18px]" />
          {on && <span className="font-medium">Thinking</span>}
        </button>
      </span>
    </Tooltip>
  );
}

/**
 * Effort level (Low / Medium / High / Extra / Max) for models that take one. "Default" sends
 * nothing; a level the model lacks uses the nearest lower one it has.
 */
export function EffortMenu({ model }: { model: ModelInfo | undefined }) {
  const effort = useUi((s) => s.effort);
  const setEffort = useUi((s) => s.setEffort);
  const caps = model?.capabilities;
  if (!caps || caps.effort.length === 0) return null;
  const applied = effectiveEffort(caps, effort);
  const fallback = caps.defaultEffort ? EFFORT_LABELS[caps.defaultEffort] : null;
  const shown = applied ? EFFORT_LABELS[applied] : "Default";
  const title = applied
    ? `Effort: ${EFFORT_LABELS[applied]}`
    : `Effort: model default${fallback ? ` (${fallback})` : ""}`;

  return (
    <MenuRoot>
      <Tooltip label={title}>
        <MenuTrigger asChild>
          <button
            type="button"
            aria-label={title}
            className={cn(
              "inline-flex h-8 items-center gap-1 rounded-lg px-2 text-sm transition-colors data-[state=open]:bg-hover",
              applied ? "text-fg" : "text-fg-muted hover:bg-hover hover:text-fg",
            )}
          >
            <Gauge className="size-[18px]" />
            <span>{shown}</span>
            <ChevronDown className="size-3.5" />
          </button>
        </MenuTrigger>
      </Tooltip>
      <MenuContent align="start" side="top" className="w-72">
        <MenuLabel>Effort</MenuLabel>
        <MenuRadioGroup
          value={applied ?? "default"}
          onValueChange={(v) => setEffort(v === "default" ? null : (v as Effort))}
        >
          <MenuRadioItem value="default">
            <span className="flex min-w-0 flex-1 flex-col">
              <span>Default{fallback ? ` (${fallback})` : ""}</span>
              <span className="text-xs text-fg-subtle">What the model uses on its own</span>
            </span>
          </MenuRadioItem>
          <MenuSeparator />
          {caps.effort.map((level) => (
            <MenuRadioItem key={level} value={level}>
              <span className="flex min-w-0 flex-1 flex-col">
                <span>{EFFORT_LABELS[level]}</span>
                <span className="text-xs text-fg-subtle">{EFFORT_HINTS[level]}</span>
              </span>
            </MenuRadioItem>
          ))}
        </MenuRadioGroup>
      </MenuContent>
    </MenuRoot>
  );
}
