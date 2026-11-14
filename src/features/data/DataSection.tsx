import { Archive, FolderOpen, LoaderCircle, Trash2 } from "lucide-react";
import { useState, type ReactNode } from "react";
import type { DataExport } from "@/bindings";
import { formatBytes } from "@/features/attachments/limits";
import { DialogButtons, Modal, field } from "@/features/projects/ProjectDialogs";
import { errorMessage } from "@/lib/api";
import { cn } from "@/lib/cn";
import { toast } from "@/lib/toast";
import {
  openDataFolder,
  revealDataExport,
  useDataLocations,
  useDeleteAll,
  useExportAll,
} from "./queries";

/** The word the delete dialog asks for (the backend checks it too). */
const CONFIRM_WORD = "delete";

/** Settings → Data: where Oxide keeps things, "Export all" and "Delete all". */
export function DataSection() {
  const { data: where } = useDataLocations();
  const exportAll = useExportAll();
  const [exported, setExported] = useState<DataExport | null>(null);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const fail = (e: unknown) => toast(errorMessage(e), "error");

  return (
    <section aria-label="Data" className="flex max-w-2xl flex-col gap-8">
      <Block title="Where your data is">
        {where ? (
          <dl className="grid grid-cols-[5rem_1fr] gap-x-4 gap-y-1 text-sm">
            <Row label="Data" value={where.data} />
            {where.config !== where.data && <Row label="Config" value={where.config} />}
            <Row label="Logs" value={where.logs} />
            {where.dataBytes !== null && (
              <Row label="Size" value={formatBytes(where.dataBytes)} testId="data-size" />
            )}
          </dl>
        ) : (
          <p className="text-sm text-fg-muted">Reading…</p>
        )}
        <p className="text-sm text-fg-subtle">
          API keys, sign-ins and pairing tokens are kept in the system keychain, not in these
          folders.
        </p>
        <div>
          <Button
            icon={<FolderOpen className="size-4" />}
            label="Show folder"
            onClick={() => void openDataFolder().catch(fail)}
          />
        </div>
      </Block>

      <Block title="Export all">
        <p className="text-base text-fg-muted">
          One zip with every chat, project, memory, artifact, scheduled task and setting (as JSON),
          your attachments and skills, and the MCP config with its secrets removed. API keys are not
          included.
        </p>
        <div className="flex flex-wrap items-center gap-2">
          <Button
            icon={
              exportAll.isPending ? (
                <LoaderCircle className="size-4 animate-spin" />
              ) : (
                <Archive className="size-4" />
              )
            }
            label={exportAll.isPending ? "Exporting…" : "Export all…"}
            disabled={exportAll.isPending}
            onClick={() =>
              exportAll.mutate(undefined, {
                onSuccess: (done) => {
                  if (done) setExported(done);
                },
                onError: fail,
              })
            }
          />
        </div>
        {exported && (
          <div
            role="status"
            className="flex flex-wrap items-center gap-x-3 gap-y-1 rounded-lg border border-line bg-raised p-3 text-sm"
          >
            <span className="min-w-0 break-all text-fg">
              Saved {formatBytes(exported.bytes)}: {exported.tables} tables ({exported.rows} rows)
              and {exported.files} files.
            </span>
            <button
              type="button"
              onClick={() => void revealDataExport().catch(fail)}
              className="text-accent hover:underline"
            >
              Show in folder
            </button>
            {exported.skipped.length > 0 && (
              <p className="w-full text-fg-muted">Left out: {exported.skipped.join("; ")}</p>
            )}
          </div>
        )}
      </Block>

      <Block title="Delete all data" danger>
        <p className="text-base text-fg-muted">
          Removes every chat, project, memory, artifact, skill, extension, scheduled task and
          setting, the managed Python and the sandboxes, and Oxide’s keychain entries. Oxide then
          restarts as if newly installed. This can’t be undone, so export first if you may want
          anything back.
        </p>
        <div>
          <Button
            danger
            icon={<Trash2 className="size-4" />}
            label="Delete all data…"
            onClick={() => setConfirmOpen(true)}
          />
        </div>
      </Block>

      <DeleteAllDialog open={confirmOpen} onOpenChange={setConfirmOpen} />
    </section>
  );
}

function DeleteAllDialog({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const [typed, setTyped] = useState("");
  const del = useDeleteAll();
  const ready = typed.trim().toLowerCase() === CONFIRM_WORD;
  const close = (o: boolean) => {
    if (del.isPending || del.isSuccess) return;
    if (!o) setTyped("");
    onOpenChange(o);
  };
  return (
    <Modal open={open} onOpenChange={close} title="Delete all data?">
      {del.isSuccess ? (
        <p role="status" className="flex items-center gap-2 text-base text-fg">
          <LoaderCircle className="size-4 animate-spin" />
          Everything was deleted. Oxide is restarting…
          {del.data.leftovers.length > 0 && " A few files were in use; they go at the next start."}
        </p>
      ) : (
        <form
          className="flex flex-col gap-4"
          onSubmit={(e) => {
            e.preventDefault();
            if (ready) del.mutate(typed);
          }}
        >
          <p className="text-base text-fg-muted">
            Every chat, project, setting, file and API key Oxide keeps is removed, and Oxide
            restarts. Type <strong className="text-fg">{CONFIRM_WORD}</strong> to confirm.
          </p>
          <input
            aria-label={`Type ${CONFIRM_WORD} to confirm`}
            data-autofocus=""
            value={typed}
            onChange={(e) => setTyped(e.target.value)}
            className={cn(field, "h-9")}
            autoComplete="off"
            spellCheck={false}
          />
          {del.error && (
            <p role="alert" className="text-base text-danger">
              {errorMessage(del.error)}
            </p>
          )}
          <DialogButtons
            onCancel={() => close(false)}
            submitLabel={del.isPending ? "Deleting…" : "Delete everything"}
            disabled={!ready || del.isPending}
            danger
          />
        </form>
      )}
    </Modal>
  );
}

function Block({
  title,
  danger,
  children,
}: {
  title: string;
  danger?: boolean;
  children: ReactNode;
}) {
  return (
    <div className={cn("flex flex-col gap-3", danger && "rounded-xl border border-line p-4")}>
      <h3 className={cn("text-md font-medium", danger ? "text-danger" : "text-fg")}>{title}</h3>
      {children}
    </div>
  );
}

function Row({ label, value, testId }: { label: string; value: string; testId?: string }) {
  return (
    <div className="contents">
      <dt className="text-fg-subtle">{label}</dt>
      <dd data-testid={testId} className="min-w-0 break-all font-mono text-xs text-fg">
        {value}
      </dd>
    </div>
  );
}

function Button({
  label,
  icon,
  danger,
  disabled,
  onClick,
}: {
  label: string;
  icon?: ReactNode;
  danger?: boolean;
  disabled?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
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
