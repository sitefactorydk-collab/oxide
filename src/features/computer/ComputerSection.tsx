import { AlertTriangle, Keyboard, Trash2 } from "lucide-react";
import type { ComputerStatus } from "@/bindings";
import { Toggle } from "@/features/models/ModelsSection";
import { SmallButton } from "@/features/sandbox/SandboxSection";
import { DEFAULT_SETTINGS, useSetComputerSettings, useSettings } from "@/features/settings/queries";
import { useComputerActivity, useComputerStatus, useStopComputer } from "./queries";

/** Settings → Computer use: on or off, the screen models use, and the apps every chat may use. */
export function ComputerSection() {
  const { data } = useSettings();
  const computer = (data ?? DEFAULT_SETTINGS).computer;
  const set = useSetComputerSettings();
  const { data: status } = useComputerStatus();
  const active = useComputerActivity();
  const stop = useStopComputer();
  if (status && !status.supported) {
    return (
      <div className="flex max-w-2xl flex-col gap-4">
        <p className="text-base text-fg-muted">
          Computer use works on Windows for now. On this system models can still use the browser and
          Chrome.
        </p>
      </div>
    );
  }
  return (
    <div className="flex max-w-2xl flex-col gap-6">
      <p className="text-base text-fg-muted">
        Models that can see images can use your computer like a person: they take screenshots, move
        and click the mouse, type and press keys. Each chat asks before it starts, and again for
        every app it uses. Oxide never types into password, one-time code or payment fields, never
        types card or bank account numbers, and never uses itself or a password manager.
      </p>
      <Toggle
        label="Let models use the computer"
        hint="Chats can ask to control the computer (switch it off per chat in the + menu). Scheduled runs ask too, even when set to run without asking."
        checked={computer.enabled}
        onChange={(enabled) => set({ enabled })}
      />
      <PanicCard status={status} enabled={computer.enabled} />
      {active.length > 0 && (
        <div
          role="status"
          className="flex items-center justify-between gap-4 rounded-xl border border-accent bg-raised px-4 py-3"
        >
          <span className="text-md text-fg">
            {active.length === 1
              ? "A chat is controlling the computer."
              : `${active.length} chats are controlling the computer.`}
          </span>
          <SmallButton
            label="Stop"
            danger
            disabled={stop.isPending}
            onClick={() => stop.mutate()}
          />
        </div>
      )}
      <section aria-label="Screen" className="flex flex-col gap-3 border-t border-line pt-5">
        <h3 className="text-md font-semibold text-fg">Screen</h3>
        <label className="flex flex-col gap-1.5">
          <span className="text-sm text-fg-muted">The screen models see and use</span>
          <select
            aria-label="Screen"
            value={computer.display ?? ""}
            onChange={(e) => set({ display: e.target.value === "" ? null : e.target.value })}
            className="h-9 w-full max-w-md rounded-lg border border-line bg-raised px-2.5 text-md text-fg"
          >
            <option value="">The main screen</option>
            {(status?.displays ?? []).map((d) => (
              <option key={d.id} value={d.id}>
                {d.name} ({d.width}×{d.height}){d.primary ? " · main" : ""}
              </option>
            ))}
          </select>
        </label>
        {status?.error && <p className="text-sm text-danger">{status.error}</p>}
        <Toggle
          label="Minimize Oxide while a chat acts"
          hint="So models see what's behind Oxide's window. It comes back when a chat asks you something and when the chat is done."
          checked={computer.hideOxide}
          onChange={(hideOxide) => set({ hideOxide })}
        />
        <Toggle
          label="Claude uses its own computer tool"
          hint="Claude models were trained on Anthropic's computer tool. Off: they get the same tool as other models."
          checked={computer.nativeAnthropic}
          onChange={(nativeAnthropic) => set({ nativeAnthropic })}
        />
      </section>
      <section aria-label="Apps" className="flex flex-col gap-3 border-t border-line pt-5">
        <h3 className="text-md font-semibold text-fg">Apps every chat may use</h3>
        <p className="text-sm text-fg-subtle">
          Other apps ask in each chat. Add an app here with “Always allow in every chat” on its
          approval card.
        </p>
        {computer.alwaysAllowedApps.length === 0 ? (
          <p className="text-sm text-fg-muted">None yet.</p>
        ) : (
          <ul className="flex flex-col divide-y divide-line rounded-xl border border-line">
            {computer.alwaysAllowedApps.map((app) => (
              <li key={app.id} className="flex items-center justify-between gap-3 px-3 py-2">
                <span className="min-w-0">
                  <span className="text-md text-fg">{app.name}</span>
                  <span className="ml-2 font-mono text-xs text-fg-subtle">{app.id}</span>
                </span>
                <SmallButton
                  label="Remove"
                  icon={<Trash2 aria-hidden className="size-3.5" />}
                  onClick={() =>
                    set({
                      alwaysAllowedApps: computer.alwaysAllowedApps.filter((a) => a.id !== app.id),
                    })
                  }
                />
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}

function PanicCard({ status, enabled }: { status: ComputerStatus | undefined; enabled: boolean }) {
  const shortcut = status?.panicShortcut ?? "Ctrl+Alt+Esc";
  return (
    <div className="flex items-start gap-3 rounded-xl border border-line bg-raised px-4 py-3">
      {enabled && status?.panicError ? (
        <AlertTriangle aria-hidden className="mt-0.5 size-4 shrink-0 text-danger" />
      ) : (
        <Keyboard aria-hidden className="mt-0.5 size-4 shrink-0 text-fg-muted" />
      )}
      <div className="text-sm">
        <p className="text-fg">
          While a chat controls the computer, a bar at the top of the screen shows what it does.
          Press <kbd className="rounded border border-line px-1 font-mono text-xs">{shortcut}</kbd>{" "}
          or the bar&apos;s Stop to stop every chat at once.
        </p>
        {enabled && status?.panicError && <p className="mt-1 text-danger">{status.panicError}</p>}
      </div>
    </div>
  );
}
