import { ChevronDown } from "lucide-react";
import { useMemo } from "react";
import {
  MenuContent,
  MenuLabel,
  MenuRadioGroup,
  MenuRadioItem,
  MenuRoot,
  MenuSeparator,
  MenuTrigger,
} from "@/components/ui/Menu";
import { MODEL_CATALOG, modelLabel, type ModelOption } from "@/features/models/catalog";
import { useUi } from "@/stores/ui";

/** Model pill in the composer. Static list until Phase 3 discovers real models per provider. */
export function ModelPicker() {
  const modelId = useUi((s) => s.modelId);
  const setModelId = useUi((s) => s.setModelId);

  const groups = useMemo(() => {
    const map = new Map<ModelOption["provider"], ModelOption[]>();
    for (const m of MODEL_CATALOG) map.set(m.provider, [...(map.get(m.provider) ?? []), m]);
    return [...map.entries()];
  }, []);

  return (
    <MenuRoot>
      <MenuTrigger asChild>
        <button
          type="button"
          aria-label={`Model: ${modelLabel(modelId)}`}
          className="inline-flex h-8 items-center gap-1 rounded-lg px-2.5 text-base text-fg-muted hover:bg-hover hover:text-fg data-[state=open]:bg-hover"
        >
          {modelLabel(modelId)}
          <ChevronDown className="size-3.5" />
        </button>
      </MenuTrigger>
      <MenuContent align="end" className="w-64">
        <MenuRadioGroup value={modelId} onValueChange={setModelId}>
          {groups.map(([provider, models], i) => (
            <div key={provider}>
              {i > 0 && <MenuSeparator />}
              <MenuLabel>{provider}</MenuLabel>
              {models.map((m) => (
                <MenuRadioItem key={m.id} value={m.id}>
                  <span className="flex-1">{m.label}</span>
                  {m.note && <span className="text-xs text-fg-subtle">{m.note}</span>}
                </MenuRadioItem>
              ))}
            </div>
          ))}
        </MenuRadioGroup>
        <MenuSeparator />
        <p className="px-2.5 py-1.5 text-xs text-fg-subtle">
          Only models with native tool calling are listed.
        </p>
      </MenuContent>
    </MenuRoot>
  );
}
