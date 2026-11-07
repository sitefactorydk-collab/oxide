import {
  AlertTriangle,
  ChevronDown,
  ChevronRight,
  FolderOpen,
  Puzzle,
  Settings2,
  Trash2,
  Upload,
} from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { commands, type Extension, type ExtensionInfo, type ExtensionPreview } from "@/bindings";
import { ConfirmDialog, Modal } from "@/features/projects/ProjectDialogs";
import { useMcpServers } from "@/features/mcp/queries";
import { STATUS_LABEL } from "@/features/mcp/status";
import { Button, StatusDot, Switch } from "@/features/mcp/ui";
import { useOpenSettings } from "@/features/settings/useOpenSettings";
import { errorMessage } from "@/lib/api";
import { cn } from "@/lib/cn";
import { toast } from "@/lib/toast";
import { ExtensionForm } from "./ExtensionForm";
import { missingRequired } from "./fields";
import {
  discardExtension,
  pickPaths,
  useConfigureExtension,
  useExtensions,
  useInstallExtension,
  usePreviewExtension,
  useSetExtensionEnabled,
  useUninstallExtension,
  type ExtensionValues,
} from "./queries";
import { useExtensionDrop } from "./store";

const SERVER_TYPE: Record<ExtensionInfo["serverType"], string> = {
  node: "Node.js",
  python: "Python",
  binary: "Program",
  uv: "Python (uv)",
};

/** Settings → Extensions: install `.mcpb` bundles, configure, switch off, update, remove. */
export function ExtensionsSection() {
  const { data: extensions, isPending } = useExtensions();
  const preview = usePreviewExtension();
  const [opened, setOpened] = useState<ExtensionPreview | null>(null);
  const pending = useExtensionDrop((s) => s.pending);

  const { mutate: openBundle } = preview;
  const open = useCallback(
    (path: string) => {
      openBundle(path, {
        onSuccess: setOpened,
        onError: (e) => toast(`Couldn’t open the extension: ${errorMessage(e)}`, "error"),
      });
    },
    [openBundle],
  );

  // A bundle dropped on the window (or a chat) opens here.
  useEffect(() => {
    if (pending === null) return;
    const p = useExtensionDrop.getState().take();
    if (p) open(p);
  }, [pending, open]);

  const browse = async () => {
    try {
      const [p] = await pickPaths("extension");
      if (p) open(p);
    } catch (e) {
      toast(errorMessage(e), "error");
    }
  };

  return (
    <div className="flex max-w-3xl flex-col gap-3">
      <div className="mb-2 flex flex-wrap gap-2">
        <Button primary onClick={() => void browse()} disabled={preview.isPending}>
          <Upload />
          {preview.isPending ? "Opening…" : "Install extension…"}
        </Button>
        <Button onClick={() => void commands.openExtensionsFolder(null)}>
          <FolderOpen />
          Open extensions folder
        </Button>
      </div>
      <p className="-mt-1 mb-2 text-sm text-fg-subtle">
        Desktop extensions are .mcpb files that bundle an MCP server. You can also drop one on
        Oxide’s window. Their servers show up in Connectors.
      </p>

      {isPending ? (
        <p className="text-fg-muted">Loading…</p>
      ) : extensions.length === 0 ? (
        <div className="rounded-xl border border-dashed border-line-strong p-6 text-center text-fg-muted">
          <Puzzle aria-hidden className="mx-auto mb-2 size-6 text-fg-subtle" />
          <p>No extensions yet.</p>
          <p className="mt-1 text-sm text-fg-subtle">Install a .mcpb file to add one.</p>
        </div>
      ) : (
        <ul aria-label="Installed extensions" className="flex flex-col gap-3">
          {extensions.map((e) => (
            <ExtensionCard key={e.name} ext={e} />
          ))}
        </ul>
      )}

      {opened && (
        <InstallDialog
          preview={opened}
          onClose={(installed) => {
            if (!installed) discardExtension(opened.stagingId);
            setOpened(null);
          }}
        />
      )}
    </div>
  );
}

function ExtIcon({ info, className }: { info: ExtensionInfo | null; className?: string }) {
  return info?.icon ? (
    <img src={info.icon} alt="" className={cn("size-10 shrink-0 rounded-lg", className)} />
  ) : (
    <span
      aria-hidden
      className={cn(
        "flex size-10 shrink-0 items-center justify-center rounded-lg bg-sunken text-fg-subtle",
        className,
      )}
    >
      <Puzzle className="size-5" />
    </span>
  );
}

function Details({ info }: { info: ExtensionInfo }) {
  const links = [
    info.homepage && { label: "Website", url: info.homepage },
    info.repository && { label: "Source", url: info.repository },
    info.authorUrl && { label: info.author, url: info.authorUrl },
  ].filter(Boolean) as Array<{ label: string; url: string }>;
  return (
    <div className="flex flex-col gap-3 text-base">
      {info.longDescription && (
        <p className="whitespace-pre-line text-fg-muted">{info.longDescription}</p>
      )}
      <dl className="grid grid-cols-[max-content_1fr] gap-x-4 gap-y-1 text-sm">
        <dt className="text-fg-subtle">Runs as</dt>
        <dd className="text-fg">
          {SERVER_TYPE[info.serverType]}
          {Object.entries(info.runtimes)
            .map(([k, v]) => ` · ${k} ${v ?? ""}`)
            .join("")}
        </dd>
        {info.license && (
          <>
            <dt className="text-fg-subtle">License</dt>
            <dd className="text-fg">{info.license}</dd>
          </>
        )}
        <dt className="text-fg-subtle">Manifest</dt>
        <dd className="text-fg">{info.manifestVersion}</dd>
      </dl>
      {info.tools.length > 0 && (
        <div>
          <p className="mb-1 text-sm font-medium text-fg-muted">
            Tools{info.toolsGenerated ? " (it may add more when it runs)" : ""}
          </p>
          <ul className="flex flex-col gap-0.5 text-sm">
            {info.tools.map((t) => (
              <li key={t.name}>
                <span className="font-mono text-fg">{t.name}</span>
                {t.description && <span className="text-fg-subtle"> · {t.description}</span>}
              </li>
            ))}
          </ul>
        </div>
      )}
      {info.prompts.length > 0 && (
        <p className="text-sm text-fg-muted">
          Prompts: {info.prompts.map((p) => p.name).join(", ")}
        </p>
      )}
      {links.length > 0 && (
        <p className="flex flex-wrap gap-3 text-sm">
          {links.map((l) => (
            <button
              key={l.url}
              type="button"
              onClick={() => void commands.openExternal(l.url)}
              className="text-accent underline-offset-2 hover:underline"
            >
              {l.label}
            </button>
          ))}
        </p>
      )}
    </div>
  );
}

function ExtensionCard({ ext }: { ext: Extension }) {
  const { data: servers } = useMcpServers();
  const server = servers.find((s) => s.name === ext.name);
  const setEnabled = useSetExtensionEnabled();
  const uninstall = useUninstallExtension();
  const openSettings = useOpenSettings();
  const [expanded, setExpanded] = useState(false);
  const [configuring, setConfiguring] = useState(false);
  const [confirm, setConfirm] = useState(false);
  const info = ext.info;
  const title = info?.displayName ?? ext.name;
  const ready = ext.status === "ready";

  return (
    <li className="rounded-xl border border-line bg-raised p-4 shadow-sm">
      <div className="flex items-start gap-3">
        <ExtIcon info={info} />
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-baseline gap-x-2">
            <span className="text-md font-medium text-fg">{title}</span>
            <span className="text-sm text-fg-subtle">
              {ext.version}
              {info ? ` · ${info.author}` : ""}
            </span>
          </div>
          {info && <p className="mt-0.5 text-base text-fg-muted">{info.description}</p>}
          <div className="mt-2 flex flex-wrap items-center gap-2 text-sm">
            {ready && server ? (
              <button
                type="button"
                onClick={() => openSettings("connectors")}
                className="inline-flex items-center gap-1.5 text-fg-muted hover:text-fg"
                title="Show in Connectors"
              >
                <StatusDot status={server.status} />
                {STATUS_LABEL[server.status]}
                {server.status === "running" && server.tools > 0
                  ? ` · ${server.tools} tool${server.tools === 1 ? "" : "s"}`
                  : ""}
              </button>
            ) : ready ? (
              <span className="text-fg-subtle">{ext.enabled ? "Starting…" : "Off"}</span>
            ) : (
              <span className="inline-flex items-center gap-1.5 text-danger">
                <AlertTriangle aria-hidden className="size-3.5" />
                {ext.status === "needsSetup"
                  ? "Needs setup"
                  : ext.status === "unsupported"
                    ? "Not for this computer"
                    : "Can’t be used"}
              </span>
            )}
          </div>
          {(ext.problem ?? (server?.status === "error" ? server.error : null)) && (
            <p className="mt-1 text-sm text-fg-muted">{ext.problem ?? server?.error}</p>
          )}
        </div>
        <Switch
          label={`Turn ${title} ${ext.enabled ? "off" : "on"}`}
          checked={ext.enabled}
          disabled={setEnabled.isPending}
          onChange={(enabled) =>
            setEnabled
              .mutateAsync({ name: ext.name, enabled })
              .catch((e: unknown) => toast(errorMessage(e), "error"))
          }
        />
      </div>
      <div className="mt-3 flex flex-wrap items-center gap-1">
        <button
          type="button"
          aria-expanded={expanded}
          onClick={() => setExpanded((x) => !x)}
          className="inline-flex h-8 items-center gap-1 rounded-lg px-2 text-sm text-fg-muted hover:bg-hover hover:text-fg"
        >
          {expanded ? <ChevronDown className="size-4" /> : <ChevronRight className="size-4" />}
          Details
        </button>
        {info && (
          <button
            type="button"
            onClick={() => setConfiguring(true)}
            className="inline-flex h-8 items-center gap-1.5 rounded-lg px-2 text-sm text-fg-muted hover:bg-hover hover:text-fg"
          >
            <Settings2 className="size-4" />
            Settings
          </button>
        )}
        <button
          type="button"
          onClick={() => void commands.openExtensionsFolder(ext.name)}
          className="inline-flex h-8 items-center gap-1.5 rounded-lg px-2 text-sm text-fg-muted hover:bg-hover hover:text-fg"
        >
          <FolderOpen className="size-4" />
          Folder
        </button>
        <button
          type="button"
          onClick={() => setConfirm(true)}
          className="ml-auto inline-flex h-8 items-center gap-1.5 rounded-lg px-2 text-sm text-danger hover:bg-danger-soft"
        >
          <Trash2 className="size-4" />
          Uninstall
        </button>
      </div>
      {expanded && info && (
        <div className="mt-3 border-t border-line pt-3">
          <Details info={info} />
        </div>
      )}
      {configuring && info && (
        <ConfigureDialog ext={ext} info={info} onClose={() => setConfiguring(false)} />
      )}
      <ConfirmDialog
        open={confirm}
        onOpenChange={setConfirm}
        title={`Uninstall ${title}?`}
        body="Its server stops, and its files, settings and saved keys are removed."
        confirmLabel="Uninstall"
        onConfirm={() =>
          uninstall
            .mutateAsync(ext.name)
            .then(() => toast(`${title} was uninstalled.`))
            .catch((e: unknown) => toast(errorMessage(e), "error"))
        }
      />
    </li>
  );
}

function ConfigureDialog({
  ext,
  info,
  onClose,
}: {
  ext: Extension;
  info: ExtensionInfo;
  onClose: () => void;
}) {
  const [values, setValues] = useState<ExtensionValues>({ ...ext.values });
  const configure = useConfigureExtension();
  const missing = missingRequired(info.fields, values, ext.secretsSet);
  return (
    <Modal
      open
      onOpenChange={(o) => !o && onClose()}
      title={`${info.displayName} settings`}
      description="Sensitive values are kept in the system keychain."
      wide
    >
      <form
        noValidate
        className="flex min-h-0 flex-col gap-5"
        onSubmit={(e) => {
          e.preventDefault();
          configure
            .mutateAsync({ name: ext.name, values })
            .then(() => {
              toast("Settings saved.");
              onClose();
            })
            .catch(() => {});
        }}
      >
        <div className="min-h-0 overflow-y-auto">
          <ExtensionForm
            fields={info.fields}
            values={values}
            secretsSet={ext.secretsSet}
            onChange={setValues}
          />
        </div>
        {configure.error && (
          <p role="alert" className="text-base text-danger">
            {errorMessage(configure.error)}
          </p>
        )}
        <FormButtons
          onCancel={onClose}
          submitLabel={configure.isPending ? "Saving…" : "Save"}
          disabled={configure.isPending || missing.length > 0}
          missing={missing}
        />
      </form>
    </Modal>
  );
}

function FormButtons({
  onCancel,
  submitLabel,
  disabled,
  missing,
}: {
  onCancel: () => void;
  submitLabel: string;
  disabled: boolean;
  missing: string[];
}) {
  return (
    <div className="flex items-center justify-end gap-2">
      {missing.length > 0 && (
        <p className="mr-auto text-sm text-fg-subtle">Fill in: {missing.join(", ")}</p>
      )}
      <button
        type="button"
        onClick={onCancel}
        className="h-9 rounded-lg px-3.5 text-base text-fg-muted hover:bg-hover"
      >
        Cancel
      </button>
      <button
        type="submit"
        disabled={disabled}
        className="h-9 rounded-lg bg-fg px-3.5 text-base font-medium text-app transition-opacity hover:opacity-90 disabled:opacity-40"
      >
        {submitLabel}
      </button>
    </div>
  );
}

function sizeText(bytes: number): string {
  if (bytes < 1024 * 1024) return `${Math.max(1, Math.round(bytes / 1024))} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function InstallDialog({
  preview,
  onClose,
}: {
  preview: ExtensionPreview;
  onClose: (installed: boolean) => void;
}) {
  const { info } = preview;
  const [values, setValues] = useState<ExtensionValues>({ ...preview.values });
  const install = useInstallExtension();
  const missing = missingRequired(info.fields, values, preview.secretsSet);
  const update = preview.installedVersion !== null;
  return (
    <Modal
      open
      onOpenChange={(o) => !o && onClose(false)}
      title={update ? `Update ${info.displayName}` : `Install ${info.displayName}`}
      wide
    >
      <form
        noValidate
        className="flex min-h-0 flex-col gap-5"
        onSubmit={(e) => {
          e.preventDefault();
          install
            .mutateAsync({ stagingId: preview.stagingId, values })
            .then((ext) => {
              toast(
                ext.status === "ready"
                  ? `${info.displayName} is installed.`
                  : `${info.displayName} is installed. ${ext.problem ?? ""}`,
              );
              onClose(true);
            })
            .catch(() => {});
        }}
      >
        <div className="flex min-h-0 flex-col gap-5 overflow-y-auto">
          <div className="flex items-start gap-3">
            <ExtIcon info={info} className="size-12" />
            <div className="min-w-0">
              <p className="text-base text-fg">{info.description}</p>
              <p className="mt-1 text-sm text-fg-subtle">
                {update
                  ? `Version ${preview.installedVersion} → ${info.version}`
                  : `Version ${info.version}`}{" "}
                · by {info.author} · {preview.files} files, {sizeText(preview.size)}
              </p>
            </div>
          </div>
          {preview.warnings.map((w) => (
            <p
              key={w}
              role="alert"
              className="flex gap-2 rounded-lg bg-danger-soft px-3 py-2 text-base text-fg"
            >
              <AlertTriangle aria-hidden className="mt-0.5 size-4 shrink-0 text-danger" />
              {w}
            </p>
          ))}
          <p className="rounded-lg bg-sunken px-3 py-2 text-sm text-fg-muted">
            Extensions run a program on your computer with your permissions. Install only ones you
            trust. Their tools still ask before they run, like other connectors.
          </p>
          <Details info={info} />
          {info.fields.length > 0 && (
            <section aria-label="Extension settings" className="border-t border-line pt-4">
              <h3 className="mb-3 text-md font-medium text-fg">Settings</h3>
              <ExtensionForm
                fields={info.fields}
                values={values}
                secretsSet={preview.secretsSet}
                onChange={setValues}
              />
            </section>
          )}
        </div>
        {install.error && (
          <p role="alert" className="text-base text-danger">
            {errorMessage(install.error)}
          </p>
        )}
        <FormButtons
          onCancel={() => onClose(false)}
          submitLabel={install.isPending ? "Installing…" : update ? "Update" : "Install"}
          disabled={install.isPending || missing.length > 0}
          missing={missing}
        />
      </form>
    </Modal>
  );
}
