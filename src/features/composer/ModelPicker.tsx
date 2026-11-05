import { ChevronDown } from "lucide-react";
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
import { findModel, isUsable, modelNameFromId } from "@/features/models/models";
import { useModels } from "@/features/models/queries";
import { useSettings } from "@/features/settings/queries";
import { useOpenSettings } from "@/features/settings/useOpenSettings";

/**
 * Model pill in the composer: the models of every enabled endpoint, grouped by endpoint.
 * Models without native tool calling are hidden, or shown greyed out when Settings → Models
 * says so.
 */
export function ModelPicker({
  value,
  onChange,
}: {
  value: string;
  onChange: (id: string) => void;
}) {
  const { data: lists, isPending, refetch } = useModels();
  const { data: settings } = useSettings();
  const openSettings = useOpenSettings();
  const showUnsupported = settings?.models.showUnsupported ?? false;
  const label = findModel(lists, value)?.label ?? modelNameFromId(value);
  const anyUsable = lists.some((l) => l.models.some(isUsable));

  return (
    <MenuRoot
      onOpenChange={(open) => {
        if (open) void refetch();
      }}
    >
      <MenuTrigger asChild>
        <button
          type="button"
          aria-label={`Model: ${label}`}
          className="inline-flex h-8 max-w-56 items-center gap-1 rounded-lg px-2.5 text-base text-fg-muted hover:bg-hover hover:text-fg data-[state=open]:bg-hover"
        >
          <span className="truncate">{label}</span>
          <ChevronDown className="size-3.5 shrink-0" />
        </button>
      </MenuTrigger>
      <MenuContent align="end" className="max-h-[min(70vh,560px)] w-72 overflow-y-auto">
        <MenuRadioGroup value={value} onValueChange={onChange}>
          {lists.map((list, i) => {
            const models = list.models.filter((m) => showUnsupported || isUsable(m));
            if (models.length === 0 && !list.error) return null;
            return (
              <div key={list.providerId} role="group" aria-label={list.providerName}>
                {i > 0 && <MenuSeparator />}
                <MenuLabel>{list.providerName}</MenuLabel>
                {list.error && (
                  <p className="px-2.5 pb-1.5 text-xs leading-4 text-fg-subtle">{list.error}</p>
                )}
                {models.map((m) => (
                  <MenuRadioItem key={m.id} value={m.id} disabled={!isUsable(m)}>
                    <span className="min-w-0 flex-1 truncate" title={m.model}>
                      {m.label}
                    </span>
                    {!isUsable(m) ? (
                      <span className="text-xs text-fg-subtle">No tools</span>
                    ) : m.capabilities.thinking !== "none" ? (
                      <span className="text-xs text-fg-subtle">Thinking</span>
                    ) : null}
                  </MenuRadioItem>
                ))}
              </div>
            );
          })}
        </MenuRadioGroup>
        {isPending && <p className="px-2.5 py-2 text-sm text-fg-subtle">Loading models…</p>}
        {!isPending && !anyUsable && (
          <p className="px-2.5 py-2 text-sm text-fg-muted">No models available yet.</p>
        )}
        <MenuSeparator />
        <MenuItem onSelect={() => openSettings(anyUsable ? "models" : "providers")}>
          {anyUsable ? "Model settings…" : "Set up a provider…"}
        </MenuItem>
        <p className="px-2.5 py-1.5 text-xs text-fg-subtle">
          Only models with native tool calling can be picked.
        </p>
      </MenuContent>
    </MenuRoot>
  );
}
