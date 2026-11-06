import { useEffect, useState } from "react";
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
  const [text, setText] = useState(String(value));
  useEffect(() => setText(String(value)), [value]);
  const commit = () => {
    const n = Math.round(Number(text));
    const next = Number.isFinite(n) ? Math.min(MAX_STEPS, Math.max(1, n)) : value;
    setText(String(next));
    if (next !== value) onChange(next);
  };
  return (
    <label className="flex flex-col gap-2">
      <span className="text-md font-medium text-fg">Most tool steps per reply</span>
      <input
        type="number"
        inputMode="numeric"
        min={1}
        max={MAX_STEPS}
        value={text}
        onChange={(e) => setText(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter") commit();
        }}
        className="h-10 w-40 rounded-lg border border-line bg-raised px-3 text-md text-fg outline-none focus:border-accent"
      />
      <span className="text-sm text-fg-subtle">
        A step is one round of the model calling tools and getting their results. When a reply
        reaches the limit, Oxide stops it and says so. 1–{MAX_STEPS}.
      </span>
    </label>
  );
}
