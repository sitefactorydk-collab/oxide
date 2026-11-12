import { Check, Download, LoaderCircle, Trash2, X } from "lucide-react";
import { useEffect, useState } from "react";
import type { BrowserSettings, BrowserStatus } from "@/bindings";
import { NumberSetting } from "@/components/NumberSetting";
import { Toggle } from "@/features/models/ModelsSection";
import { allModels, isUsable } from "@/features/models/models";
import { useModels } from "@/features/models/queries";
import { SmallButton } from "@/features/sandbox/SandboxSection";
import { DEFAULT_SETTINGS, useSetBrowserSettings, useSettings } from "@/features/settings/queries";
import { errorMessage } from "@/lib/api";
import { cn } from "@/lib/cn";
import { toast } from "@/lib/toast";
import { useBrowserSetup, useBrowserStatus, useRemoveChromium } from "./queries";

/** Settings → Browser: what is installed, which browser chats use, and how they may use it. */
export function BrowserSection() {
  const { data } = useSettings();
  const browser = (data ?? DEFAULT_SETTINGS).browser;
  const set = useSetBrowserSettings();
  const { data: status } = useBrowserStatus();
  return (
    <div className="flex max-w-2xl flex-col gap-6">
      <p className="text-base text-fg-muted">
        The browser lets models open web pages, read them, click and type, take screenshots, and
        hand whole browsing tasks to an autonomous agent (Browser-Use). Each chat gets a browser of
        its own with a fresh profile, so your own sign-ins aren&apos;t used unless you connect your
        Chrome below. You see what it does as screenshots in the chat.
      </p>
      <SetupCard status={status} settings={browser} />
      <Toggle
        label="Let models use a browser"
        hint="Adds the browser's tools to chats (switch it off per chat in the + menu). The first use installs the browser packages, and Chromium if needed."
        checked={browser.enabled}
        onChange={(enabled) => set({ enabled })}
      />
      <Source status={status} settings={browser} onChange={set} />
      <Toggle
        label="Show the browser window"
        hint="Off: the browser runs out of sight and the chat shows screenshots. On: its window opens on your screen while chats use it."
        checked={browser.showWindow}
        onChange={(showWindow) => set({ showWindow })}
      />
      <AgentModel settings={browser} onChange={set} />
      <section aria-label="Limits" className="flex flex-col gap-5 border-t border-line pt-5">
        <h3 className="text-md font-semibold text-fg">Limits</h3>
        <NumberSetting
          label="Steps per browsing task"
          hint="Most actions one browsing task may take before it stops and reports. 1–200."
          value={browser.maxSteps}
          min={1}
          max={200}
          onChange={(maxSteps) => set({ maxSteps })}
        />
        <NumberSetting
          label="Time limit per browsing task (seconds)"
          hint="A task still running after this is stopped. 10–3600."
          value={browser.taskTimeoutSecs}
          min={10}
          max={3600}
          onChange={(taskTimeoutSecs) => set({ taskTimeoutSecs })}
        />
        <NumberSetting
          label="Close when idle (minutes)"
          hint="A chat's browser nobody used for this long is closed. It opens again when needed."
          value={browser.idleMinutes}
          min={1}
          max={1440}
          onChange={(idleMinutes) => set({ idleMinutes })}
        />
      </section>
      <section aria-label="Approvals" className="flex flex-col gap-4 border-t border-line pt-5">
        <h3 className="text-md font-semibold text-fg">Approvals</h3>
        <Toggle
          label="Ask before opening pages, clicking, typing and browsing tasks"
          hint="Each one asks, with Allow always for this chat. Reading the page and screenshots never ask. Models are told never to type passwords or payment details, and not to buy, post or send anything you didn't ask for."
          checked={browser.ask}
          onChange={(ask) => set({ ask })}
        />
      </section>
    </div>
  );
}

const STEP_LABELS: Record<NonNullable<BrowserStatus["step"]>, string> = {
  python: "Setting up Python",
  packages: "Installing the browser packages",
  chromium: "Downloading Chromium",
};

function SetupCard({
  status,
  settings,
}: {
  status: BrowserStatus | undefined;
  settings: BrowserSettings;
}) {
  const setup = useBrowserSetup();
  const remove = useRemoveChromium();
  if (!status) return <p className="text-base text-fg-muted">Checking the browser…</p>;
  const run = (chromium: boolean) =>
    setup.mutate(chromium, { onError: (e) => toast(errorMessage(e), "error") });
  const failed = !status.installing && status.error !== null;
  const title = status.installing
    ? status.step
      ? STEP_LABELS[status.step]
      : "Setting up…"
    : failed
      ? "Setting up the browser failed"
      : status.packages
        ? "The browser is ready"
        : "Not set up yet";
  const usingText =
    settings.source === "cdp"
      ? `Your Chrome at ${settings.cdpUrl}`
      : (status.using ?? "Chromium (downloaded the first time a chat uses the browser)");
  return (
    <section
      aria-label="Browser setup"
      className={cn(
        "flex flex-col gap-2 rounded-xl border p-4",
        failed ? "border-danger bg-danger-soft" : "border-line bg-raised",
      )}
    >
      <div className="flex items-center gap-2 text-md font-medium text-fg" role="status">
        {status.installing ? (
          <LoaderCircle className="size-4 animate-spin" />
        ) : failed ? (
          <X className="size-4 text-danger" />
        ) : status.packages ? (
          <Check className="size-4 text-success" />
        ) : null}
        {title}
        {status.installing && status.percent !== null && (
          <span className="text-sm tabular-nums text-fg-subtle">{status.percent}%</span>
        )}
      </div>
      {status.installing && status.percent !== null && (
        <div
          role="progressbar"
          aria-label="Download"
          aria-valuenow={status.percent}
          aria-valuemin={0}
          aria-valuemax={100}
          className="h-1.5 overflow-hidden rounded-full bg-sunken"
        >
          <div className="h-full bg-accent" style={{ width: `${status.percent}%` }} />
        </div>
      )}
      {failed && <p className="whitespace-pre-wrap text-sm text-fg">{status.error}</p>}
      {(status.installing || failed) && status.log.length > 0 && (
        <pre
          aria-label="Setup output"
          className="max-h-40 overflow-auto rounded-lg bg-sunken px-3 py-2 font-mono text-xs leading-5 text-fg-muted"
        >
          {status.log.slice(-12).join("\n")}
        </pre>
      )}
      {!status.installing && (
        <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-sm">
          <dt className="text-fg-subtle">Chats use</dt>
          <dd className="text-fg">{usingText}</dd>
          {status.versions.length > 0 && (
            <>
              <dt className="text-fg-subtle">Packages</dt>
              <dd className="text-fg">
                {status.versions.map((v) => `${v.name} ${v.version}`).join(" · ")}
              </dd>
            </>
          )}
          <dt className="text-fg-subtle">On this computer</dt>
          <dd className="text-fg">
            {status.installed.length > 0
              ? status.installed.map((b) => b.name).join(", ")
              : "No Chrome or Edge found"}
          </dd>
          <dt className="text-fg-subtle">Oxide&apos;s Chromium</dt>
          <dd className="text-fg">{status.chromium ? "Downloaded" : "Not downloaded"}</dd>
        </dl>
      )}
      <div className="flex flex-wrap gap-2">
        {!status.installing && (!status.packages || failed) && (
          <SmallButton
            icon={<Download className="size-4" />}
            label={failed ? "Try again" : "Set up now"}
            disabled={setup.isPending}
            onClick={() => run(false)}
          />
        )}
        {!status.installing && !status.chromium && settings.source !== "cdp" && (
          <SmallButton
            icon={<Download className="size-4" />}
            label="Download Chromium"
            disabled={setup.isPending}
            onClick={() => run(true)}
          />
        )}
        {!status.installing && status.chromium && (
          <SmallButton
            icon={<Trash2 className="size-4" />}
            label="Remove Chromium"
            danger
            disabled={remove.isPending}
            onClick={() =>
              remove.mutate(undefined, {
                onSuccess: () => toast("Oxide's Chromium was removed."),
                onError: (e) => toast(errorMessage(e), "error"),
              })
            }
          />
        )}
      </div>
    </section>
  );
}

function Source({
  status,
  settings,
  onChange,
}: {
  status: BrowserStatus | undefined;
  settings: BrowserSettings;
  onChange: (patch: Partial<BrowserSettings>) => void;
}) {
  const found = status?.installed[0]?.name;
  return (
    <section aria-label="Which browser" className="flex flex-col gap-3 border-t border-line pt-5">
      <h3 className="text-md font-semibold text-fg">Which browser</h3>
      <div role="radiogroup" aria-label="Browser" className="flex flex-col gap-2">
        <Radio
          checked={settings.source === "auto"}
          onSelect={() => onChange({ source: "auto" })}
          label="A browser on this computer"
          hint={`${found ? `${found}, ` : "Chrome or Edge, "}with a profile of Oxide's own. Without one, Oxide downloads Chromium.`}
        />
        <Radio
          checked={settings.source === "bundled"}
          onSelect={() => onChange({ source: "bundled" })}
          label="Chromium downloaded by Oxide"
          hint="Always the same build, kept with Oxide's Python (about 150 MB, downloaded once)."
        />
        <Radio
          checked={settings.source === "cdp"}
          onSelect={() => onChange({ source: "cdp" })}
          label="My own Chrome (connect over remote debugging)"
          hint="Recommended for searching: search engines often answer a hidden browser with checks for robots, and Oxide never tries to get around them. In your own visible Chrome you can complete a check yourself. Oxide works in a tab of its own (searches use a tab you keep on Google, if there is one) and never closes your tabs."
        />
      </div>
      {settings.source === "cdp" && (
        <div className="flex flex-col gap-2 pl-6">
          <TextSetting
            label="Chrome's debugging address"
            hint="Only addresses on this computer (127.0.0.1 or localhost)."
            value={settings.cdpUrl}
            onSave={(cdpUrl) => onChange({ cdpUrl })}
          />
          <p className="text-sm text-fg-subtle">
            Start Chrome like this (Chrome needs a profile folder other than your default one for
            remote debugging). The first time, search something on Google yourself in that Chrome
            and answer its cookie page:
          </p>
          <pre className="overflow-auto rounded-lg bg-sunken px-3 py-2 font-mono text-xs text-fg">
            chrome.exe --remote-debugging-port=9222 --user-data-dir=%LOCALAPPDATA%\OxideChrome
          </pre>
          <p className="text-sm text-danger">
            While it runs, any program on this computer can control that Chrome. Close it when
            you&apos;re done.
          </p>
        </div>
      )}
    </section>
  );
}

function AgentModel({
  settings,
  onChange,
}: {
  settings: BrowserSettings;
  onChange: (patch: Partial<BrowserSettings>) => void;
}) {
  const { data: lists } = useModels();
  const usable = allModels(lists).filter(isUsable);
  return (
    <label className="flex flex-col gap-2 border-t border-line pt-5">
      <span className="text-md font-medium text-fg">Model for browsing tasks</span>
      <select
        aria-label="Model for browsing tasks"
        value={settings.agentModel ?? ""}
        onChange={(e) => onChange({ agentModel: e.target.value || null })}
        className="h-10 w-96 max-w-full rounded-lg border border-line bg-raised px-3 text-md text-fg outline-none focus:border-accent"
      >
        <option value="">The chat&apos;s model</option>
        {usable.map((m) => (
          <option key={m.id} value={m.id}>
            {m.label} (
            {lists.find((l) => l.providerId === m.providerId)?.providerName ?? m.providerId})
          </option>
        ))}
      </select>
      <span className="text-sm text-fg-subtle">
        Browser-Use calls this model through the same endpoint and key Oxide uses. It works best
        with a capable model that can see images.
      </span>
    </label>
  );
}

function Radio({
  checked,
  onSelect,
  label,
  hint,
}: {
  checked: boolean;
  onSelect: () => void;
  label: string;
  hint: string;
}) {
  return (
    <label className="flex cursor-pointer items-start gap-2">
      <input
        type="radio"
        className="mt-1 accent-accent"
        checked={checked}
        onChange={onSelect}
        aria-label={label}
      />
      <span>
        <span className="block text-md text-fg">{label}</span>
        <span className="block text-sm text-fg-subtle">{hint}</span>
      </span>
    </label>
  );
}

/** A text field that saves when it loses focus or on Enter. */
function TextSetting({
  label,
  hint,
  value,
  onSave,
}: {
  label: string;
  hint: string;
  value: string;
  onSave: (v: string) => void;
}) {
  const [text, setText] = useState(value);
  useEffect(() => setText(value), [value]);
  const commit = () => {
    const v = text.trim();
    if (v && v !== value) onSave(v);
    else setText(value);
  };
  return (
    <label className="flex flex-col gap-1.5">
      <span className="text-md font-medium text-fg">{label}</span>
      <input
        aria-label={label}
        value={text}
        spellCheck={false}
        onChange={(e) => setText(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter") commit();
          if (e.key === "Escape") setText(value);
        }}
        className="h-10 rounded-lg border border-line bg-raised px-3 font-mono text-sm text-fg outline-none focus:border-accent"
      />
      <span className="text-sm text-fg-subtle">{hint}</span>
    </label>
  );
}
