import { NumberSetting } from "@/components/NumberSetting";
import { Toggle } from "@/features/models/ModelsSection";
import { DEFAULT_SETTINGS, useSetToolSettings, useSettings } from "@/features/settings/queries";

const MAX_STEPS = 200;

/** Settings → Permissions: approvals and the tool loop. */
export function PermissionsSection() {
  const { data } = useSettings();
  const tools = (data ?? DEFAULT_SETTINGS).tools;
  const set = useSetToolSettings();
  return (
    <div className="flex max-w-2xl flex-col gap-6">
      <p className="text-base text-fg-muted">
        Oxide asks before a connector’s tool runs. Each request shows the tool, its server and its
        arguments, with <em>Allow once</em>, <em>Allow always for this chat</em> and <em>Deny</em>.
      </p>
      <Toggle
        label="Run read-only connector tools without asking"
        hint="Tools whose server marks them as read-only (readOnlyHint). The hint comes from the server, so only turn this on for servers you trust."
        checked={tools.autoAllowReadOnly}
        onChange={(autoAllowReadOnly) => set({ autoAllowReadOnly })}
      />
      <Toggle
        label="Ask before web search and web fetch"
        hint="Oxide’s own web tools only read public pages, so they run without asking unless this is on."
        checked={tools.askBeforeWeb}
        onChange={(askBeforeWeb) => set({ askBeforeWeb })}
      />
      <StepsField value={tools.maxSteps} onChange={(maxSteps) => set({ maxSteps })} />
    </div>
  );
}

function StepsField({ value, onChange }: { value: number; onChange: (n: number) => void }) {
  return (
    <NumberSetting
      label="Most tool steps per reply"
      hint={`A step is one round of the model calling tools and getting their results. When a reply reaches the limit, Oxide stops it and says so. 1–${MAX_STEPS}.`}
      value={value}
      min={1}
      max={MAX_STEPS}
      onChange={onChange}
    />
  );
}
