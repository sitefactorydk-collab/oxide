import { Check, Keyboard } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { commands, type ShortcutStatus } from "@/bindings";
import { Toggle } from "@/features/models/ModelsSection";
import { DEFAULT_SETTINGS, useSetDesktopSettings, useSettings } from "@/features/settings/queries";
import { errorMessage, unwrap } from "@/lib/api";
import { cn } from "@/lib/cn";
import { isMacLike } from "@/lib/platform";
import { toast } from "@/lib/toast";
import { useDesktopStatus } from "./queries";
import { DEFAULT_QUICK_ENTRY_SHORTCUT, formatShortcut, shortcutFromKeys } from "./shortcut";

/** Settings → Desktop: tray, quick entry, start at login, notifications and links. */
export function DesktopSection() {
  const { data } = useSettings();
  const desktop = (data ?? DEFAULT_SETTINGS).desktop;
  const set = useSetDesktopSettings();
  const status = useDesktopStatus().data;

  return (
    <div className="flex max-w-2xl flex-col gap-6">
      <Toggle
        label="Keep Oxide in the tray when its window is closed"
        hint="Closing the window leaves Oxide running, so quick entry and replies in progress keep working. Quit from the tray icon. When this is off, closing the window quits Oxide."
        checked={desktop.closeToTray}
        onChange={(closeToTray) => set({ closeToTray })}
      />
      <div className="flex flex-col gap-3">
        <Toggle
          label="Quick entry"
          hint="A shortcut that works in any app opens a small box to ask Oxide something. Press Enter and the chat opens in Oxide’s window. It can attach a screenshot of your screen."
          checked={desktop.quickEntry}
          onChange={(quickEntry) => set({ quickEntry })}
        />
        {desktop.quickEntry && (
          <ShortcutField
            value={desktop.quickEntryShortcut}
            status={status?.shortcut}
            onChange={(quickEntryShortcut) => set({ quickEntryShortcut })}
          />
        )}
      </div>
      <div className="flex flex-col gap-2">
        <Toggle
          label="Start Oxide when you log in"
          hint="Oxide starts in the tray without opening its window."
          checked={desktop.launchAtLogin}
          onChange={(launchAtLogin) => set({ launchAtLogin })}
        />
        {status?.launchAtLoginError && (
          <p role="alert" className="text-sm text-danger">
            {status.launchAtLoginError}
          </p>
        )}
      </div>
      <div className="flex flex-col gap-2">
        <Toggle
          label="Notifications"
          hint="Tell me when a reply finishes or a tool needs my approval while Oxide isn’t in front."
          checked={desktop.notifications}
          onChange={(notifications) => set({ notifications })}
        />
        <div>
          <button
            type="button"
            onClick={() =>
              void unwrap(commands.testNotification()).then(
                () =>
                  toast(
                    "Test notification sent. Don’t see it? Check Windows’ notification settings.",
                  ),
                (e: unknown) => toast(errorMessage(e), "error"),
              )
            }
            className="h-8 rounded-lg border border-line px-3 text-sm text-fg hover:bg-hover"
          >
            Send a test notification
          </button>
        </div>
      </div>
      <LinksNote scheme={status?.deepLinkScheme ?? "oxide"} devBuild={status?.devBuild ?? false} />
    </div>
  );
}

function ShortcutField({
  value,
  status,
  onChange,
}: {
  value: string;
  status: ShortcutStatus | undefined;
  onChange: (shortcut: string) => void;
}) {
  const [recording, setRecording] = useState(false);
  const boxRef = useRef<HTMLButtonElement>(null);
  const active = status?.active === value;
  const error = status?.wanted === value ? status.error : null;

  // While recording, every key goes to the recorder first: a window capture listener runs
  // before the Settings dialog's own (document capture) Esc handler, so Esc cancels the
  // recording instead of closing Settings.
  const latest = useRef({ value, onChange });
  latest.current = { value, onChange };
  useEffect(() => {
    if (!recording) return;
    boxRef.current?.focus();
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Escape" && !e.ctrlKey && !e.altKey && !e.metaKey) {
        setRecording(false);
        return;
      }
      const next = shortcutFromKeys(e);
      if (next) {
        setRecording(false);
        if (next !== latest.current.value) latest.current.onChange(next);
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [recording]);

  return (
    <div className="ml-0.5 flex flex-col gap-2 border-l-2 border-line pl-4">
      <div className="text-md font-medium text-fg">Shortcut</div>
      <div className="flex flex-wrap items-center gap-2">
        <button
          ref={boxRef}
          type="button"
          aria-label={recording ? "Press the new shortcut" : "Quick entry shortcut"}
          onClick={() => setRecording(true)}
          onBlur={() => setRecording(false)}
          className={cn(
            "inline-flex h-9 min-w-40 items-center gap-2 rounded-lg border px-3 font-mono text-base",
            recording
              ? "border-accent bg-accent-soft text-fg"
              : "border-line bg-raised text-fg hover:bg-hover",
          )}
        >
          <Keyboard className="size-4 shrink-0 text-fg-subtle" />
          {recording ? "Press keys…" : formatShortcut(value, isMacLike)}
        </button>
        {!recording && (
          <button
            type="button"
            onClick={() => setRecording(true)}
            className="h-9 rounded-lg px-3 text-base text-fg-muted hover:bg-hover hover:text-fg"
          >
            Change
          </button>
        )}
        {!recording && value !== DEFAULT_QUICK_ENTRY_SHORTCUT && (
          <button
            type="button"
            onClick={() => onChange(DEFAULT_QUICK_ENTRY_SHORTCUT)}
            className="h-9 rounded-lg px-3 text-base text-fg-muted hover:bg-hover hover:text-fg"
          >
            Reset to {formatShortcut(DEFAULT_QUICK_ENTRY_SHORTCUT, isMacLike)}
          </button>
        )}
        {active && !recording && (
          <span className="inline-flex items-center gap-1 text-sm text-success">
            <Check className="size-4" /> Active
          </span>
        )}
      </div>
      {recording ? (
        <p className="text-sm text-fg-subtle">
          Hold one or more modifier keys and press a key. Esc cancels.
        </p>
      ) : error ? (
        <p role="alert" className="text-sm text-danger">
          {error}
        </p>
      ) : (
        <p className="text-sm text-fg-subtle">
          Press it again, or click elsewhere, to close quick entry. Esc closes it too.
        </p>
      )}
    </div>
  );
}

function LinksNote({ scheme, devBuild }: { scheme: string; devBuild: boolean }) {
  return (
    <div className="rounded-xl border border-line bg-sunken p-4 text-sm text-fg-muted">
      <div className="mb-2 text-md font-medium text-fg">Links that open Oxide</div>
      <ul className="flex flex-col gap-1 font-mono">
        <li>{scheme}://new?q=Hello</li>
        <li>{scheme}://chat/&lt;chat id&gt;</li>
        <li>{scheme}://settings/desktop</li>
      </ul>
      {devBuild && (
        <p className="mt-3">
          This is a development build: links and start at login run the development binary, which
          needs the dev server running.
        </p>
      )}
    </div>
  );
}
