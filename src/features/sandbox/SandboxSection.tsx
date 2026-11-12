import {
  Check,
  Circle,
  Download,
  ExternalLink,
  FolderOpen,
  LoaderCircle,
  RefreshCw,
  X,
} from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";
import { commands, type DockerStatus, type SandboxSettings } from "@/bindings";
import { NumberSetting } from "@/components/NumberSetting";
import { formatBytes } from "@/features/attachments/limits";
import { Toggle } from "@/features/models/ModelsSection";
import { DEFAULT_SETTINGS, useSetSandboxSettings, useSettings } from "@/features/settings/queries";
import { errorMessage, unwrap } from "@/lib/api";
import { cn } from "@/lib/cn";
import { toast } from "@/lib/toast";
import { useDockerStatus, usePrepareSandbox, useRefreshDocker } from "./queries";

/** Settings → Sandbox: Docker's status, the images, and how sandboxes are made. */
export function SandboxSection() {
  const { data } = useSettings();
  const sandbox = (data ?? DEFAULT_SETTINGS).sandbox;
  const set = useSetSandboxSettings();
  const { data: docker } = useDockerStatus();
  return (
    <div className="flex max-w-2xl flex-col gap-6">
      <p className="text-base text-fg-muted">
        The sandbox gives each chat its own Linux container in Docker, where models can create
        projects, install packages, build and run tests without touching your computer. Its files
        live in a workspace that only that container sees. Files move in or out only when you
        approve a copy.
      </p>
      <DockerCard status={docker} />
      <Toggle
        label="Let models use a sandbox"
        hint="Adds sandbox commands, its file tools and copies in and out of shared folders to chats. Needs Docker running."
        checked={sandbox.enabled}
        onChange={(enabled) => set({ enabled })}
      />
      <Images status={docker} settings={sandbox} onChange={set} />
      <section aria-label="Limits" className="flex flex-col gap-5 border-t border-line pt-5">
        <h3 className="text-md font-semibold text-fg">Limits</h3>
        <NumberSetting
          label="CPUs"
          hint={`Most CPU cores one sandbox may use.${docker?.cpus ? ` Docker has ${docker.cpus}.` : ""}`}
          value={sandbox.cpus}
          min={1}
          max={64}
          onChange={(cpus) => set({ cpus })}
        />
        <NumberSetting
          label="Memory (MB)"
          hint={`Most memory one sandbox may use.${docker?.memoryBytes ? ` Docker has ${formatBytes(docker.memoryBytes)}.` : ""}`}
          value={sandbox.memoryMb}
          min={256}
          max={262144}
          step={256}
          onChange={(memoryMb) => set({ memoryMb })}
        />
        <NumberSetting
          label="Time limit per command (seconds)"
          hint="A command still running after this is stopped, with everything it started. 1–3600."
          value={sandbox.timeoutSecs}
          min={1}
          max={3600}
          onChange={(timeoutSecs) => set({ timeoutSecs })}
        />
        <NumberSetting
          label="Stop when idle (minutes)"
          hint="A sandbox nobody used for this long is stopped. Its files stay; it starts again when needed."
          value={sandbox.idleMinutes}
          min={1}
          max={1440}
          onChange={(idleMinutes) => set({ idleMinutes })}
        />
        <Toggle
          label="Internet access"
          hint="Off: sandboxes have no network at all. On: they can download packages (pip, apt-get, npm) — and anything else, so only turn it on when you need it. New limits and network settings apply to a chat's sandbox the next time it is used: its container is made again (files in /workspace stay; installed packages don't)."
          checked={sandbox.network}
          onChange={(network) => set({ network })}
        />
      </section>
      <section aria-label="Approvals" className="flex flex-col gap-4 border-t border-line pt-5">
        <h3 className="text-md font-semibold text-fg">Approvals</h3>
        <Toggle
          label="Ask before commands and file changes in the sandbox"
          hint="Like connector tools: each one asks, with Allow always for this chat. Off: they run without asking, since they can't reach your computer. Copies in and out of the sandbox always ask."
          checked={sandbox.ask}
          onChange={(ask) => set({ ask })}
        />
      </section>
      <Storage settings={sandbox} onChange={set} />
    </div>
  );
}

function DockerCard({ status }: { status: DockerStatus | undefined }) {
  const refresh = useRefreshDocker();
  const recheck = () =>
    refresh.mutate(undefined, { onError: (e) => toast(errorMessage(e), "error") });
  if (!status) return <p className="text-base text-fg-muted">Checking Docker…</p>;
  const ready = status.state === "ready";
  const title = {
    unknown: "Checking Docker…",
    missing: "Docker isn't installed",
    stopped: "Docker isn't running",
    ready: `Docker ${status.version ?? ""} is running`,
  }[status.state];
  return (
    <section
      aria-label="Docker"
      className={cn(
        "flex flex-col gap-2 rounded-xl border p-4",
        ready ? "border-line bg-raised" : "border-danger bg-danger-soft",
      )}
    >
      <div className="flex items-center gap-2 text-md font-medium text-fg" role="status">
        {ready ? (
          <Check className="size-4 text-success" />
        ) : status.state === "unknown" ? (
          <LoaderCircle className="size-4 animate-spin" />
        ) : (
          <X className="size-4 text-danger" />
        )}
        {title}
      </div>
      {status.message && !ready && <p className="text-sm text-fg">{status.message}</p>}
      {ready && (status.cpus || status.memoryBytes) && (
        <p className="text-sm text-fg-subtle">
          {status.cpus ? `${status.cpus} CPUs` : ""}
          {status.cpus && status.memoryBytes ? " · " : ""}
          {status.memoryBytes ? `${formatBytes(status.memoryBytes)} memory` : ""}
          {status.program ? ` · ${status.program}` : ""}
        </p>
      )}
      <div className="flex flex-wrap gap-2">
        {status.state === "missing" && (
          <SmallButton
            icon={<ExternalLink className="size-4" />}
            label="Get Docker"
            onClick={() =>
              void unwrap(commands.openExternal(status.installUrl)).catch((e: unknown) =>
                toast(errorMessage(e), "error"),
              )
            }
          />
        )}
        <SmallButton
          icon={<RefreshCw className={cn("size-4", refresh.isPending && "animate-spin")} />}
          label="Check again"
          disabled={refresh.isPending}
          onClick={recheck}
        />
      </div>
    </section>
  );
}

/** The two images, whether they're downloaded, and their names. */
function Images({
  status,
  settings,
  onChange,
}: {
  status: DockerStatus | undefined;
  settings: SandboxSettings;
  onChange: (patch: Partial<SandboxSettings>) => void;
}) {
  const prepare = usePrepareSandbox();
  const images = status?.images ?? [];
  const missing = images.some((i) => !i.present);
  const pulling = status?.pulling ?? null;
  return (
    <section aria-label="Images" className="flex flex-col gap-4 border-t border-line pt-5">
      <h3 className="text-md font-semibold text-fg">Images</h3>
      <TextSetting
        label="Image for commands"
        hint="Any Linux image with sh. The default has Python 3.12, pip, gcc, make and git."
        value={settings.image}
        onSave={(image) => onChange({ image })}
      />
      <TextSetting
        label="Image for the file tools"
        hint="The reference filesystem MCP server's image."
        value={settings.filesImage}
        onSave={(filesImage) => onChange({ filesImage })}
      />
      {images.length > 0 && (
        <ul aria-label="Downloaded images" className="flex flex-col gap-1 text-sm">
          {images.map((i) => (
            <li key={`${i.role}:${i.name}`} className="flex items-center gap-2">
              {pulling === i.name ? (
                <LoaderCircle className="size-3.5 animate-spin" />
              ) : i.present ? (
                <Check className="size-3.5 text-success" />
              ) : (
                <Circle className="size-3.5 text-fg-subtle" />
              )}
              <span className="text-fg-subtle">{i.role}:</span>
              <span className="font-mono text-fg">{i.name}</span>
              <span className="text-fg-subtle">
                {pulling === i.name
                  ? "downloading…"
                  : i.present
                    ? i.sizeBytes
                      ? formatBytes(i.sizeBytes)
                      : "downloaded"
                    : "not downloaded"}
              </span>
            </li>
          ))}
        </ul>
      )}
      {status?.state === "ready" && missing && (
        <div className="flex flex-col gap-1">
          <SmallButton
            icon={
              prepare.isPending ? (
                <LoaderCircle className="size-4 animate-spin" />
              ) : (
                <Download className="size-4" />
              )
            }
            label={prepare.isPending ? "Downloading…" : "Download images"}
            disabled={prepare.isPending}
            onClick={() =>
              prepare.mutate(undefined, {
                onSuccess: () => toast("The sandbox images are ready."),
                onError: (e) => toast(errorMessage(e), "error"),
              })
            }
          />
          <span className="text-sm text-fg-subtle">
            Otherwise they download the first time a chat uses the sandbox.
          </span>
        </div>
      )}
    </section>
  );
}

function Storage({
  settings,
  onChange,
}: {
  settings: SandboxSettings;
  onChange: (patch: Partial<SandboxSettings>) => void;
}) {
  const pick = async () => {
    try {
      const picked = await unwrap(commands.pickPaths("folder", false));
      if (picked[0]) onChange({ folder: picked[0] });
    } catch (e) {
      toast(errorMessage(e), "error");
    }
  };
  return (
    <section
      aria-label="Where workspaces live"
      className="flex flex-col gap-3 border-t border-line pt-5"
    >
      <h3 className="text-md font-semibold text-fg">Where workspaces live</h3>
      <div role="radiogroup" aria-label="Workspace storage" className="flex flex-col gap-2">
        <Radio
          checked={settings.storage === "volume"}
          onSelect={() => onChange({ storage: "volume" })}
          label="In Docker (a volume per chat)"
          hint="Fast, and kept inside Docker's own disk."
        />
        <Radio
          checked={settings.storage === "folder"}
          onSelect={() => onChange({ storage: "folder" })}
          label="In a folder on this computer"
          hint="One folder per chat, which you can open in your file manager. Only that folder is mounted into the sandbox."
        />
      </div>
      {settings.storage === "folder" && (
        <div className="flex items-center gap-2 pl-6">
          <span className="min-w-0 flex-1 truncate font-mono text-sm text-fg-muted">
            {settings.folder || "Oxide's data folder (default)"}
          </span>
          <SmallButton
            icon={<FolderOpen className="size-4" />}
            label="Choose…"
            onClick={() => void pick()}
          />
          {settings.folder && (
            <SmallButton label="Use default" onClick={() => onChange({ folder: "" })} />
          )}
        </div>
      )}
      <p className="text-sm text-fg-subtle">
        This applies to sandboxes made from now on; existing ones keep their workspace until you
        reset them.
      </p>
    </section>
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

export function SmallButton({
  label,
  icon,
  disabled,
  danger,
  onClick,
}: {
  label: string;
  icon?: ReactNode;
  disabled?: boolean;
  danger?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      disabled={disabled}
      onClick={onClick}
      className={cn(
        "inline-flex h-8 w-fit items-center gap-1.5 rounded-lg border border-line px-2.5 text-sm transition-colors disabled:opacity-50",
        danger ? "text-danger hover:bg-danger-soft" : "text-fg hover:bg-hover",
      )}
    >
      {icon}
      {label}
    </button>
  );
}
