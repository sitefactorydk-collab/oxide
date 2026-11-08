import { Check, Circle, FolderOpen, LoaderCircle, RefreshCw, Trash2, X } from "lucide-react";
import { useState, type ReactNode } from "react";
import type { PythonStatus, PythonStep } from "@/bindings";
import { formatBytes } from "@/features/attachments/limits";
import { ConfirmDialog } from "@/features/projects/ProjectDialogs";
import { errorMessage } from "@/lib/api";
import { cn } from "@/lib/cn";
import { toast } from "@/lib/toast";
import {
  openPythonFolder,
  usePythonRepair,
  usePythonReset,
  usePythonSetup,
  usePythonStatus,
} from "./queries";

/** The steps of setting up, in order, with the words the page shows for each. */
const PYTHON_STEPS: { step: PythonStep; label: string }[] = [
  { step: "uv", label: "Find uv" },
  { step: "python", label: "Install Python" },
  { step: "venv", label: "Create the environment" },
  { step: "packages", label: "Install the packages" },
  { step: "check", label: "Check that they import" },
];

/** Settings → Advanced: the managed Python environment (setup, repair, reset). */
export function PythonSection() {
  const { data: status } = usePythonStatus();
  const setup = usePythonSetup();
  const repair = usePythonRepair();
  const reset = usePythonReset();
  const [confirmReset, setConfirmReset] = useState(false);

  if (!status) return <p className="text-base text-fg-muted">Reading the Python environment…</p>;

  const installing = status.state === "installing";
  const fail = (e: unknown) => toast(errorMessage(e), "error");

  return (
    <section aria-label="Managed Python" className="flex max-w-2xl flex-col gap-5">
      <p className="text-base text-fg-muted">
        Oxide keeps its own Python, installed with <code>uv</code>, so models can run code with
        pandas, numpy, matplotlib and openpyxl without anything installed on your computer. It is
        set up the first time a model runs Python, or here.
      </p>

      <StatusLine status={status} />
      {installing && <Steps status={status} />}
      {status.state === "failed" && status.error && (
        <p role="alert" className="rounded-lg border border-danger p-3 text-sm text-danger">
          {status.error}
        </p>
      )}
      {status.state === "ready" && <Details status={status} />}
      {status.log.length > 0 && (status.state === "failed" || installing) && (
        <pre
          aria-label="uv output"
          className="max-h-48 overflow-auto whitespace-pre-wrap rounded-lg border border-line bg-sunken p-3 font-mono text-xs text-fg-muted"
        >
          {status.log.join("\n")}
        </pre>
      )}

      <div className="flex flex-wrap gap-2">
        {(status.state === "missing" || status.state === "failed") && (
          <ActionButton
            label={status.state === "failed" ? "Try again" : "Set up Python"}
            onClick={() => setup.mutate(undefined, { onError: fail })}
            disabled={installing || setup.isPending}
          />
        )}
        <ActionButton
          label="Repair"
          icon={<RefreshCw className="size-4" />}
          onClick={() => repair.mutate(undefined, { onError: fail })}
          disabled={installing || repair.isPending}
          hint="Builds the environment again. Downloaded Pythons and packages are reused."
        />
        <ActionButton
          label="Reset"
          icon={<Trash2 className="size-4" />}
          danger
          onClick={() => setConfirmReset(true)}
          disabled={installing || reset.isPending}
          hint="Removes the environment, the downloaded Pythons and the package cache."
        />
        <ActionButton
          label="Show folder"
          icon={<FolderOpen className="size-4" />}
          onClick={() => void openPythonFolder().catch(fail)}
        />
      </div>

      <ConfirmDialog
        open={confirmReset}
        onOpenChange={setConfirmReset}
        title="Reset the Python environment?"
        body="This removes the environment, the downloaded Pythons and the package cache. Python is downloaded and installed again the next time a model runs code. The files in your chats' working folders are kept."
        confirmLabel="Reset"
        onConfirm={() =>
          reset.mutate(undefined, {
            onSuccess: () => toast("The Python environment was removed."),
            onError: fail,
          })
        }
      />
    </section>
  );
}

function StatusLine({ status }: { status: PythonStatus }) {
  const text = {
    missing: "Not set up yet",
    installing: status.message ?? "Setting up Python…",
    ready: "Ready",
    failed: "Setup failed",
  }[status.state];
  return (
    <div className="flex items-center gap-2 text-md font-medium text-fg" role="status">
      {status.state === "installing" && <LoaderCircle className="size-4 animate-spin" />}
      {status.state === "ready" && <Check className="size-4 text-success" />}
      {status.state === "failed" && <X className="size-4 text-danger" />}
      {status.state === "missing" && <Circle className="size-4 text-fg-subtle" />}
      {text}
    </div>
  );
}

function Steps({ status }: { status: PythonStatus }) {
  const current = PYTHON_STEPS.findIndex((s) => s.step === status.step);
  return (
    <ol aria-label="Setup steps" className="flex flex-col gap-1.5 text-sm">
      {PYTHON_STEPS.map((s, i) => {
        const done = current > i;
        const active = current === i;
        return (
          <li
            key={s.step}
            aria-current={active ? "step" : undefined}
            className={cn(
              "flex items-center gap-2",
              active ? "text-fg" : done ? "text-fg-muted" : "text-fg-subtle",
            )}
          >
            {done ? (
              <Check className="size-3.5 text-success" />
            ) : active ? (
              <LoaderCircle className="size-3.5 animate-spin" />
            ) : (
              <Circle className="size-3.5" />
            )}
            {s.label}
          </li>
        );
      })}
    </ol>
  );
}

function Details({ status }: { status: PythonStatus }) {
  const rows: [string, string][] = [
    ["Python", status.pythonVersion ?? "unknown"],
    ["uv", status.uvVersion ?? "unknown"],
    ["Folder", status.folder],
    ...(status.diskBytes === null
      ? []
      : [["Disk space", formatBytes(status.diskBytes)] as [string, string]]),
  ];
  return (
    <div className="flex flex-col gap-3">
      <dl className="grid grid-cols-[6rem_1fr] gap-x-4 gap-y-1 text-sm">
        {rows.map(([k, v]) => (
          <div key={k} className="contents">
            <dt className="text-fg-subtle">{k}</dt>
            <dd className="min-w-0 break-all text-fg">{v}</dd>
          </div>
        ))}
      </dl>
      {status.packages.length > 0 && (
        <div>
          <div className="mb-1 text-sm font-medium text-fg">Packages</div>
          <ul aria-label="Installed packages" className="flex flex-wrap gap-1.5">
            {status.packages.map((p) => (
              <li
                key={p.name}
                className="rounded-md border border-line bg-raised px-2 py-0.5 font-mono text-xs text-fg-muted"
              >
                {p.name} {p.version}
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}

function ActionButton({
  label,
  icon,
  hint,
  danger,
  disabled,
  onClick,
}: {
  label: string;
  icon?: ReactNode;
  hint?: string;
  danger?: boolean;
  disabled?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      title={hint}
      disabled={disabled}
      onClick={onClick}
      className={cn(
        "inline-flex h-9 items-center gap-1.5 rounded-lg border border-line px-3 text-sm transition-colors disabled:opacity-50",
        danger ? "text-danger hover:bg-danger-soft" : "text-fg hover:bg-hover",
      )}
    >
      {icon}
      {label}
    </button>
  );
}
