import {
  ChevronRight,
  Download,
  File,
  FilePlus,
  Folder,
  FolderPlus,
  Link2,
  LoaderCircle,
  RefreshCw,
  Square,
  SquareTerminal,
  Trash2,
} from "lucide-react";
import { useState, type ReactNode } from "react";
import type { ChatSandbox, SandboxFile } from "@/bindings";
import { IconButton } from "@/components/ui/IconButton";
import { formatBytes } from "@/features/attachments/limits";
import { ConfirmDialog } from "@/features/projects/ProjectDialogs";
import { useOpenSettings } from "@/features/settings/useOpenSettings";
import { errorMessage } from "@/lib/api";
import { cn } from "@/lib/cn";
import { toast } from "@/lib/toast";
import {
  useChatSandbox,
  useCopyIntoSandbox,
  useResetSandbox,
  useSandboxFiles,
  useSaveFromSandbox,
  useStartSandbox,
  useStopSandbox,
} from "./queries";

const ROOT = "/workspace";

/**
 * The chat's Docker sandbox in the session panel: whether it runs, what runs in it, its files
 * (with Save to a folder), Copy in, Stop and Reset.
 */
export function SandboxSection({
  id,
  wrap,
}: {
  id: string;
  wrap: (title: string, aside: ReactNode, body: ReactNode) => ReactNode;
}) {
  const { data: sb } = useChatSandbox(id);
  const [path, setPath] = useState(ROOT);
  const [confirmReset, setConfirmReset] = useState(false);
  const stop = useStopSandbox(id);
  const reset = useResetSandbox(id);
  const copyIn = useCopyIntoSandbox(id);
  const openSettings = useOpenSettings();
  const fail = (e: unknown) => toast(errorMessage(e), "error");

  const aside = sb ? <StatePill sandbox={sb} /> : null;
  if (!sb) return wrap("Sandbox", aside, <p className="text-sm text-fg-subtle">Checking…</p>);

  const copy = (folder: boolean) =>
    copyIn.mutate(
      { dest: `${path}/`, folder },
      {
        onSuccess: (at) => at && toast(`Copied into the sandbox as ${at}.`),
        onError: fail,
      },
    );

  const body = (
    <div className="flex flex-col gap-2">
      {sb.error && sb.state !== "running" && (
        <p role="alert" className="rounded-lg bg-danger-soft px-2 py-1.5 text-sm text-fg">
          {sb.error}{" "}
          <button
            type="button"
            className="underline underline-offset-2"
            onClick={() => openSettings("sandbox")}
          >
            Settings → Sandbox
          </button>
        </p>
      )}
      {sb.state === "none" && !sb.error && (
        <p className="text-sm text-fg-subtle">
          Nothing yet. The sandbox starts when Oxide first runs a command or writes a file in it.
        </p>
      )}
      {sb.state === "starting" && (
        <p className="flex items-center gap-1.5 text-sm text-fg-subtle">
          <LoaderCircle className="size-3.5 animate-spin" />
          Starting (the first time, its image downloads)…
        </p>
      )}
      {sb.state !== "none" && (
        <p className="text-xs text-fg-subtle">
          {sb.image ?? "?"} · {sb.network ? "internet on" : "no network"}
          {sb.workspace
            ? ` · ${sb.storage === "folder" ? "folder" : "volume"} ${sb.workspace}`
            : ""}
        </p>
      )}
      {sb.commands.length > 0 && (
        <ul aria-label="Running commands" className="flex flex-col gap-0.5">
          {sb.commands.map((c) => (
            <li key={c.id} className="flex items-center gap-2 text-sm text-fg">
              <SquareTerminal aria-hidden className="size-3.5 shrink-0 text-accent" />
              <span className="min-w-0 flex-1 truncate font-mono">{c.command}</span>
              <LoaderCircle aria-label="Running" className="size-3.5 animate-spin text-fg-subtle" />
            </li>
          ))}
        </ul>
      )}
      {sb.state === "running" && <Files id={id} path={path} onOpen={setPath} />}
      {sb.state === "stopped" && <Stopped id={id} path={path} />}
      {sb.state !== "none" && (
        <div className="flex flex-wrap gap-1 pt-1">
          <PanelButton
            icon={<FilePlus className="size-3.5" />}
            label="Copy in a file"
            disabled={copyIn.isPending}
            onClick={() => copy(false)}
          />
          <PanelButton
            icon={<FolderPlus className="size-3.5" />}
            label="Copy in a folder"
            disabled={copyIn.isPending}
            onClick={() => copy(true)}
          />
          {sb.state === "running" && (
            <PanelButton
              icon={<Square className="size-3.5" />}
              label="Stop"
              disabled={stop.isPending || sb.commands.length > 0}
              onClick={() => stop.mutate(undefined, { onError: fail })}
            />
          )}
          <PanelButton
            icon={<Trash2 className="size-3.5" />}
            label="Reset"
            danger
            disabled={reset.isPending || sb.commands.length > 0}
            onClick={() => setConfirmReset(true)}
          />
        </div>
      )}
      <ConfirmDialog
        open={confirmReset}
        onOpenChange={setConfirmReset}
        title="Reset this chat's sandbox?"
        body="This removes the sandbox and every file in its workspace. Save anything you want to keep first. The next command starts a fresh, empty sandbox."
        confirmLabel="Reset"
        onConfirm={() =>
          reset.mutate(undefined, {
            onSuccess: () => {
              setPath(ROOT);
              toast("The sandbox was reset.");
            },
            onError: fail,
          })
        }
      />
    </div>
  );
  return wrap("Sandbox", aside, body);
}

function StatePill({ sandbox }: { sandbox: ChatSandbox }) {
  const label = {
    none: "Not started",
    starting: "Starting",
    stopped: "Stopped",
    running: "Running",
  }[sandbox.state];
  return (
    <span
      className={cn(
        "rounded px-1.5 text-xs",
        sandbox.state === "running" ? "bg-accent-soft text-accent" : "bg-sunken text-fg-subtle",
      )}
    >
      {label}
    </span>
  );
}

function Stopped({ id, path }: { id: string; path: string }) {
  const start = useStartSandbox(id);
  return (
    <div className="flex flex-col gap-1">
      <p className="text-sm text-fg-subtle">
        Stopped while idle; its files are kept and it starts again when it's used.
      </p>
      <PanelButton
        icon={<RefreshCw className={cn("size-3.5", start.isPending && "animate-spin")} />}
        label="Start and show files"
        disabled={start.isPending}
        onClick={() => start.mutate(path, { onError: (e) => toast(errorMessage(e), "error") })}
      />
    </div>
  );
}

function Files({ id, path, onOpen }: { id: string; path: string; onOpen: (p: string) => void }) {
  const { data, isPending, error, refetch, isFetching } = useSandboxFiles(id, path);
  const save = useSaveFromSandbox(id);
  const crumbs = path
    .slice(1)
    .split("/")
    .map((_, i, parts) => `/${parts.slice(0, i + 1).join("/")}`);
  const saveOne = (f: SandboxFile) =>
    save.mutate(f.path, {
      onSuccess: (to) => to && toast(`Saved to ${to}`),
      onError: (e) => toast(errorMessage(e), "error"),
    });
  return (
    <div aria-label="Sandbox files" role="group" className="flex flex-col gap-1">
      <div className="flex items-center gap-1">
        <nav
          aria-label="Sandbox folder"
          className="flex min-w-0 flex-1 flex-wrap items-center text-xs"
        >
          {crumbs.map((c, i) => (
            <span key={c} className="flex items-center">
              {i > 0 && <ChevronRight aria-hidden className="size-3 text-fg-subtle" />}
              <button
                type="button"
                onClick={() => onOpen(c)}
                className={cn(
                  "rounded px-0.5 hover:bg-hover",
                  c === path ? "font-medium text-fg" : "text-fg-muted",
                )}
              >
                {c.split("/").pop()}
              </button>
            </span>
          ))}
        </nav>
        <IconButton size="sm" label="Refresh files" onClick={() => void refetch()}>
          <RefreshCw className={cn("size-3.5", isFetching && "animate-spin")} />
        </IconButton>
      </div>
      {isPending ? (
        <p className="text-sm text-fg-subtle">Reading…</p>
      ) : error ? (
        <p className="text-sm text-danger">{errorMessage(error)}</p>
      ) : !data || data.entries.length === 0 ? (
        <p className="text-sm text-fg-subtle">This folder is empty.</p>
      ) : (
        <ul aria-label="Files in the sandbox" className="flex max-h-72 flex-col overflow-y-auto">
          {data.entries.map((f) => (
            <li
              key={f.path}
              className="group flex items-center gap-1.5 rounded px-1 py-0.5 hover:bg-hover"
            >
              {f.dir ? (
                <Folder aria-hidden className="size-3.5 shrink-0 text-fg-subtle" />
              ) : f.link ? (
                <Link2 aria-hidden className="size-3.5 shrink-0 text-fg-subtle" />
              ) : (
                <File aria-hidden className="size-3.5 shrink-0 text-fg-subtle" />
              )}
              {f.dir ? (
                <button
                  type="button"
                  onClick={() => onOpen(f.path)}
                  className="min-w-0 flex-1 truncate text-left text-sm text-fg hover:underline"
                >
                  {f.name}
                </button>
              ) : (
                <span className="min-w-0 flex-1 truncate text-sm text-fg">{f.name}</span>
              )}
              {f.size !== null && (
                <span className="shrink-0 text-xs tabular-nums text-fg-subtle">
                  {formatBytes(f.size)}
                </span>
              )}
              {!f.link && (
                <IconButton
                  size="sm"
                  label={`Save ${f.name} to a folder`}
                  disabled={save.isPending}
                  onClick={() => saveOne(f)}
                >
                  <Download className="size-3.5" />
                </IconButton>
              )}
            </li>
          ))}
          {data.truncated && (
            <li className="px-1 text-xs text-fg-subtle">Only the first 500 entries are shown.</li>
          )}
        </ul>
      )}
    </div>
  );
}

function PanelButton({
  icon,
  label,
  disabled,
  danger,
  onClick,
}: {
  icon: ReactNode;
  label: string;
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
        "inline-flex h-7 items-center gap-1 rounded-md px-1.5 text-sm disabled:opacity-50",
        danger ? "text-danger hover:bg-danger-soft" : "text-fg-muted hover:bg-hover hover:text-fg",
      )}
    >
      {icon}
      {label}
    </button>
  );
}
