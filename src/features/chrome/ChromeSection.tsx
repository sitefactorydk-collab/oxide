import { Check, Copy, FolderOpen, LoaderCircle, Plug, Trash2, X } from "lucide-react";
import { useEffect, useState } from "react";
import type { ChromeSettings, ChromeStatus, PairingCode } from "@/bindings";
import { NumberSetting } from "@/components/NumberSetting";
import { Toggle } from "@/features/models/ModelsSection";
import { SmallButton } from "@/features/sandbox/SandboxSection";
import { DEFAULT_SETTINGS, useSetChromeSettings, useSettings } from "@/features/settings/queries";
import { errorMessage } from "@/lib/api";
import { cn } from "@/lib/cn";
import { toast } from "@/lib/toast";
import {
  useCancelPairing,
  useChromeStatus,
  useForgetBrowser,
  useOpenExtensionFolder,
  useStartPairing,
} from "./queries";

/** Settings → Chrome: the extension, pairing, and how models may use the user's Chrome. */
export function ChromeSection() {
  const { data } = useSettings();
  const chrome = (data ?? DEFAULT_SETTINGS).chrome;
  const set = useSetChromeSettings();
  const { data: status } = useChromeStatus();
  return (
    <div className="flex max-w-2xl flex-col gap-6">
      <p className="text-base text-fg-muted">
        With Oxide&apos;s Chrome extension, models can work in tabs of your everyday Chrome: open
        pages, read them, click, type and take screenshots, with your sign-ins. Once your Chrome is
        paired, nothing asks unless you want it to; the extension shows a banner with Stop in every
        tab Oxide works in, skips sites you block in its popup, and never types into password or
        payment fields.
      </p>
      <ConnectionCard status={status} settings={chrome} />
      <Toggle
        label="Let models use Chrome"
        hint="Oxide listens for its extension on this computer only (127.0.0.1). Chats get the Chrome tools while a paired Chrome is connected (switch it off per chat in the + menu)."
        checked={chrome.enabled}
        onChange={(enabled) => set({ enabled })}
      />
      <Install status={status} />
      <Paired status={status} />
      <section aria-label="Connection" className="flex flex-col gap-5 border-t border-line pt-5">
        <h3 className="text-md font-semibold text-fg">Connection</h3>
        <NumberSetting
          label="Port"
          hint="Where Oxide listens for the extension (127.0.0.1 only). Change it if another program uses it, and type the same port in the extension's popup."
          value={chrome.port}
          min={1024}
          max={65535}
          onChange={(port) => set({ port })}
        />
      </section>
      <section aria-label="Approvals" className="flex flex-col gap-4 border-t border-line pt-5">
        <h3 className="text-md font-semibold text-fg">Approvals</h3>
        <Toggle
          label="Ask before opening pages, clicking and typing in Chrome"
          hint="Off: a paired Chrome is all the permission Oxide needs. On: each action asks, with Allow always for this chat (reading, finding, scrolling and screenshots never ask). The extension's popup has its own switch to ask once per site, and a list of sites never to use."
          checked={chrome.ask}
          onChange={(ask) => set({ ask })}
        />
      </section>
    </div>
  );
}

function ConnectionCard({
  status,
  settings,
}: {
  status: ChromeStatus | undefined;
  settings: ChromeSettings;
}) {
  if (!status) return <p className="text-base text-fg-muted">Checking Chrome…</p>;
  const failed = settings.enabled && status.error !== null;
  const title = !settings.enabled
    ? "Off"
    : failed
      ? "Oxide can't listen for Chrome"
      : status.connected
        ? `Connected: ${status.connected}`
        : status.paired.length > 0
          ? "Waiting for Chrome"
          : "Not paired yet";
  const hint = !settings.enabled
    ? "Switch on “Let models use Chrome” to pair the extension."
    : failed
      ? status.error
      : status.connected
        ? `Chats can use Chrome now${status.sessions.length > 0 ? ` (${status.sessions.length} working in it)` : ""}.`
        : status.paired.length > 0
          ? "Open Chrome with the Oxide extension: it connects by itself within half a minute."
          : "Install the extension (below), then pair it.";
  return (
    <section
      aria-label="Chrome connection"
      className={cn(
        "flex flex-col gap-3 rounded-xl border p-4",
        failed ? "border-danger bg-danger-soft" : "border-line bg-raised",
      )}
    >
      <div className="flex items-center gap-2 text-md font-medium text-fg" role="status">
        {failed ? (
          <X className="size-4 text-danger" />
        ) : status.connected ? (
          <Check className="size-4 text-success" />
        ) : settings.enabled ? (
          <Plug className="size-4 text-fg-subtle" />
        ) : null}
        {title}
      </div>
      <p className="text-sm text-fg-muted">{hint}</p>
      {settings.enabled && status.listening && (
        <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-sm">
          <dt className="text-fg-subtle">Listening on</dt>
          <dd className="font-mono text-fg">127.0.0.1:{status.port}</dd>
          {status.extensionVersion && (
            <>
              <dt className="text-fg-subtle">Extension</dt>
              <dd className="text-fg">{status.extensionVersion}</dd>
            </>
          )}
        </dl>
      )}
      {settings.enabled && status.listening && <Pairing pairing={status.pairing} />}
    </section>
  );
}

/** "Pair a browser", then the six words to type into the extension's popup. */
function Pairing({ pairing }: { pairing: PairingCode | null }) {
  const start = useStartPairing();
  const cancel = useCancelPairing();
  const left = useMinutesLeft(pairing?.expiresAt ?? null);
  if (!pairing) {
    return (
      <SmallButton
        icon={
          start.isPending ? (
            <LoaderCircle className="size-4 animate-spin" />
          ) : (
            <Plug className="size-4" />
          )
        }
        label="Pair a browser"
        disabled={start.isPending}
        onClick={() => start.mutate(undefined, { onError: (e) => toast(errorMessage(e), "error") })}
      />
    );
  }
  const copy = () => {
    void navigator.clipboard
      ?.writeText(pairing.code)
      .then(() => toast("Code copied"))
      .catch(() => undefined);
  };
  return (
    <div
      aria-label="Pairing code"
      role="group"
      className="flex flex-col gap-2 rounded-lg bg-sunken p-3"
    >
      <p className="text-sm text-fg-muted">
        In Chrome, click the Oxide extension&apos;s icon and type these six words:
      </p>
      <div className="flex flex-wrap items-center gap-3">
        <code
          aria-label="Code"
          className="select-all rounded-md bg-raised px-3 py-1.5 font-mono text-md tracking-wide text-fg"
        >
          {pairing.code}
        </code>
        <SmallButton icon={<Copy className="size-4" />} label="Copy" onClick={copy} />
        <SmallButton label="Cancel" onClick={() => cancel.mutate()} />
      </div>
      <p className="text-xs text-fg-subtle">
        {left > 1 ? `Valid for ${left} more minutes.` : "Valid for about a minute more."} It works
        once.
      </p>
    </div>
  );
}

function useMinutesLeft(expiresAt: number | null): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (expiresAt === null) return;
    // A new code: count from now, not from when the page opened.
    setNow(Date.now());
    const t = setInterval(() => setNow(Date.now()), 15_000);
    return () => clearInterval(t);
  }, [expiresAt]);
  return expiresAt === null ? 0 : Math.max(0, Math.ceil((expiresAt - now) / 60_000));
}

function Install({ status }: { status: ChromeStatus | undefined }) {
  const open = useOpenExtensionFolder();
  const dir = status?.extensionDir ?? null;
  return (
    <section
      aria-label="Install the extension"
      className="flex flex-col gap-3 border-t border-line pt-5"
    >
      <h3 className="text-md font-semibold text-fg">Install the extension</h3>
      <ol className="list-decimal space-y-1.5 pl-5 text-sm text-fg-muted">
        <li>
          In Chrome, open <span className="font-mono text-fg">chrome://extensions</span> and switch
          on <b className="text-fg">Developer mode</b> (top right).
        </li>
        <li>
          Click <b className="text-fg">Load unpacked</b> and pick this folder:{" "}
          {dir ? (
            <span className="break-all font-mono text-fg">{dir}</span>
          ) : (
            <span className="text-fg">
              the extension isn&apos;t built yet: run{" "}
              <span className="font-mono">npm run extension</span> in Oxide&apos;s folder.
            </span>
          )}
        </li>
        <li>Pin the Oxide icon (puzzle-piece menu), click it and type the code from Pair.</li>
      </ol>
      {dir && (
        <SmallButton
          icon={<FolderOpen className="size-4" />}
          label="Open the folder"
          onClick={() =>
            open.mutate(undefined, { onError: (e) => toast(errorMessage(e), "error") })
          }
        />
      )}
    </section>
  );
}

function Paired({ status }: { status: ChromeStatus | undefined }) {
  const forget = useForgetBrowser();
  const paired = status?.paired ?? [];
  return (
    <section aria-label="Paired browsers" className="flex flex-col gap-3 border-t border-line pt-5">
      <h3 className="text-md font-semibold text-fg">Paired browsers</h3>
      {paired.length === 0 ? (
        <p className="text-sm text-fg-subtle">None yet.</p>
      ) : (
        <ul className="flex flex-col gap-2">
          {paired.map((p) => {
            const online = status?.connected !== null && status?.connected === p.name;
            return (
              <li
                key={p.id}
                className="flex items-center gap-3 rounded-lg border border-line bg-raised px-3 py-2"
              >
                <span
                  aria-hidden
                  className={cn("size-2 rounded-full", online ? "bg-success" : "bg-line-strong")}
                />
                <div className="min-w-0 flex-1">
                  <div className="truncate text-sm font-medium text-fg">{p.name}</div>
                  <div className="text-xs text-fg-subtle">
                    {online ? "Connected" : "Not connected"} · paired{" "}
                    {new Date(p.pairedAt).toLocaleDateString()}
                  </div>
                </div>
                <SmallButton
                  icon={<Trash2 className="size-4" />}
                  label="Forget"
                  danger
                  disabled={forget.isPending}
                  onClick={() => forget.mutate(p.id)}
                />
              </li>
            );
          })}
        </ul>
      )}
    </section>
  );
}
