import { useEffect, useState } from "react";
import { NumberSetting } from "@/components/NumberSetting";
import { Toggle } from "@/features/models/ModelsSection";
import {
  DEFAULT_SETTINGS,
  useSetShellSettings,
  useSetToolSettings,
  useSettings,
} from "@/features/settings/queries";
import { platform } from "@/lib/platform";

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
      <p className="text-base text-fg-muted">
        In folders you share with a chat, Oxide lists, reads and searches files without asking, and
        asks before it writes, edits, moves or deletes anything. Deleted files go to a{" "}
        <code>_to_delete</code> folder unless the chat’s panel allows deleting for good.
      </p>
      <ShellSection />
    </div>
  );
}

/** The shell tool: off by default; commands ask unless they're on the allowlist. */
function ShellSection() {
  const { data } = useSettings();
  const shell = (data ?? DEFAULT_SETTINGS).shell;
  const set = useSetShellSettings();
  const joined = shell.allowlist.join("\n");
  const [draft, setDraft] = useState(joined);
  useEffect(() => setDraft(joined), [joined]);
  const save = () => {
    const allowlist = [
      ...new Set(
        draft
          .split("\n")
          .map((l) => l.trim().replace(/\s+/g, " "))
          .filter(Boolean),
      ),
    ];
    if (allowlist.join("\n") !== joined) set({ allowlist });
  };
  return (
    <section aria-label="Shell commands" className="flex flex-col gap-4 border-t border-line pt-5">
      <h3 className="text-md font-semibold text-fg">Shell commands</h3>
      <Toggle
        label="Let models run commands in shared folders"
        hint={`Commands run in ${platform === "windows" ? "PowerShell" : "the system shell"} on this computer, inside a folder shared with the chat. Each command asks first unless it's on the allowlist below.`}
        checked={shell.enabled}
        onChange={(enabled) => set({ enabled })}
      />
      {shell.enabled && (
        <>
          <NumberSetting
            label="Time limit per command (seconds)"
            hint="A command still running after this is stopped, with whatever it started. 1–3600."
            value={shell.timeoutSecs}
            min={1}
            max={3600}
            onChange={(timeoutSecs) => set({ timeoutSecs })}
          />
          <label className="flex flex-col gap-1.5">
            <span className="text-md font-medium text-fg">Run without asking</span>
            <span className="text-sm text-fg-subtle">
              One command per line. A command runs without asking when it starts with a line here,
              word by word (<code>git status</code> allows <code>git status -s</code>, not{" "}
              <code>git push</code>). Commands that chain or redirect (<code>;</code>,{" "}
              <code>&amp;&amp;</code>, <code>|</code>, <code>&gt;</code>) always ask.
            </span>
            <textarea
              aria-label="Commands that run without asking"
              value={draft}
              rows={4}
              onChange={(e) => setDraft(e.target.value)}
              onBlur={save}
              placeholder={platform === "windows" ? "Get-ChildItem\ngit status" : "ls\ngit status"}
              className="rounded-lg border border-line bg-raised px-3 py-2 font-mono text-sm text-fg outline-none focus:border-line-strong"
            />
          </label>
        </>
      )}
    </section>
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
