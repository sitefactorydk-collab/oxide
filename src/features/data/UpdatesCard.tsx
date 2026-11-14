import { Download, LoaderCircle, RefreshCw } from "lucide-react";
import type { UpdateStatus } from "@/bindings";
import { formatBytes } from "@/features/attachments/limits";
import { Switch } from "@/features/mcp/ui";
import { DEFAULT_SETTINGS, useSettings, useUpdateSettings } from "@/features/settings/queries";
import { errorMessage } from "@/lib/api";
import { toast } from "@/lib/toast";
import { useCheckForUpdate, useInstallUpdate, useUpdateStatus } from "./queries";

/** Settings → About → Updates: check, install, and the automatic check switch. */
export function UpdatesCard() {
  const { data: status } = useUpdateStatus();
  const check = useCheckForUpdate();
  const install = useInstallUpdate();
  const settings = useSettings().data ?? DEFAULT_SETTINGS;
  const update = useUpdateSettings();
  const fail = (e: unknown) => toast(errorMessage(e), "error");

  if (!status) return null;
  const busy =
    status.phase === "checking" ||
    status.phase === "downloading" ||
    status.phase === "installing" ||
    check.isPending ||
    install.isPending;

  return (
    <section
      aria-label="Updates"
      className="mt-8 flex max-w-xl flex-col gap-3 rounded-xl border border-line p-4"
    >
      <h3 className="text-md font-medium text-fg">Updates</h3>
      <p role="status" className="flex items-center gap-2 text-base text-fg-muted">
        {busy && <LoaderCircle className="size-4 animate-spin" />}
        {describe(status)}
      </p>
      {status.phase === "available" && status.available?.notes && (
        <pre className="max-h-40 overflow-auto whitespace-pre-wrap rounded-lg border border-line bg-sunken p-3 text-sm text-fg-muted">
          {status.available.notes}
        </pre>
      )}
      {status.error && status.phase === "failed" && (
        <p role="alert" className="text-sm text-danger">
          {status.error}
        </p>
      )}
      {status.note && <p className="text-sm text-fg-subtle">{status.note}</p>}
      {status.configured && (
        <div className="flex flex-wrap items-center gap-2">
          {status.phase === "available" && status.canInstall ? (
            <button
              type="button"
              disabled={busy}
              onClick={() => install.mutate(undefined, { onError: fail })}
              className="inline-flex h-9 items-center gap-1.5 rounded-lg bg-fg px-3 text-sm font-medium text-app hover:opacity-90 disabled:opacity-50"
            >
              <Download className="size-4" />
              Install {status.available?.version} and restart
            </button>
          ) : (
            <button
              type="button"
              disabled={busy}
              onClick={() => check.mutate(undefined, { onError: fail })}
              className="inline-flex h-9 items-center gap-1.5 rounded-lg border border-line px-3 text-sm text-fg hover:bg-hover disabled:opacity-50"
            >
              <RefreshCw className="size-4" />
              Check for updates
            </button>
          )}
          <label className="ml-auto flex items-center gap-2 text-sm text-fg-muted">
            <Switch
              small
              label="Check for updates automatically"
              checked={settings.updates.checkAutomatically}
              onChange={(on) =>
                update((s) => ({ ...s, updates: { ...s.updates, checkAutomatically: on } }))
              }
            />
            Check automatically
          </label>
        </div>
      )}
    </section>
  );
}

function describe(s: UpdateStatus): string {
  const current = `Version ${s.currentVersion}`;
  switch (s.phase) {
    case "idle":
      return s.configured ? `${current}. Not checked yet.` : current;
    case "checking":
      return "Checking for updates…";
    case "upToDate":
      return `${current} is the newest.`;
    case "available":
      return `Oxide ${s.available?.version ?? ""} is available (you have ${s.currentVersion}).`;
    case "downloading":
      return s.total
        ? `Downloading… ${formatBytes(s.downloaded)} of ${formatBytes(s.total)}`
        : `Downloading… ${formatBytes(s.downloaded)}`;
    case "installing":
      return "Installing. Oxide restarts in a moment…";
    case "failed":
      return "The last update check didn’t work.";
  }
}
