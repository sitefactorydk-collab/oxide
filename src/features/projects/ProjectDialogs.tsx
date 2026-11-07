import { X } from "lucide-react";
import { Dialog } from "radix-ui";
import { useState, type ReactNode } from "react";
import type { Project } from "@/bindings";
import { IconButton } from "@/components/ui/IconButton";
import { errorMessage } from "@/lib/api";
import { cn } from "@/lib/cn";
import { formatTokens } from "./knowledge";
import { patchOf, useAddProjectText, useKnowledgeFile, useUpdateProject } from "./queries";

export const field =
  "w-full rounded-lg border border-line bg-raised px-3 text-md text-fg outline-none transition-colors focus:border-accent";

const MAX_INSTRUCTIONS = 20000;

/** A centred dialog with a title and a close button. */
export function Modal({
  open,
  onOpenChange,
  title,
  description,
  wide,
  children,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  description?: string;
  wide?: boolean;
  children: ReactNode;
}) {
  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-40 bg-overlay" />
        <Dialog.Content
          // Without a description, tell Radix there's none (it warns otherwise).
          {...(description ? {} : { "aria-describedby": undefined })}
          // Radix focuses the first button (the ✕) when nothing inside takes focus, which
          // pops its tooltip open. Keep a field or button that asked for focus, else focus
          // the dialog itself.
          onOpenAutoFocus={(e) => {
            e.preventDefault();
            const dialog = e.currentTarget as HTMLElement;
            const wanted = dialog.querySelector<HTMLElement>("[data-autofocus]");
            if (wanted) wanted.focus();
            else if (!dialog.contains(document.activeElement)) dialog.focus();
          }}
          className={cn(
            "ox-pop fixed left-1/2 top-1/2 z-50 flex max-h-[calc(100vh-48px)] -translate-x-1/2 -translate-y-1/2 flex-col rounded-2xl border border-line bg-app p-6 shadow-lg outline-none",
            wide ? "w-[min(760px,calc(100vw-48px))]" : "w-[min(520px,calc(100vw-48px))]",
          )}
        >
          <div className="mb-4 flex items-start justify-between gap-4">
            <div className="min-w-0">
              <Dialog.Title className="truncate font-serif text-xl font-normal text-fg">
                {title}
              </Dialog.Title>
              {description && (
                <Dialog.Description className="mt-1 text-base text-fg-muted">
                  {description}
                </Dialog.Description>
              )}
            </div>
            <Dialog.Close asChild>
              <IconButton size="sm" label="Close">
                <X className="size-4" />
              </IconButton>
            </Dialog.Close>
          </div>
          {open && children}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

export function DialogButtons({
  onCancel,
  submitLabel,
  disabled,
  danger,
}: {
  onCancel: () => void;
  submitLabel: string;
  disabled?: boolean;
  danger?: boolean;
}) {
  return (
    <div className="flex justify-end gap-2">
      <button
        type="button"
        onClick={onCancel}
        // A destructive confirmation starts on Cancel.
        data-autofocus={danger ? "" : undefined}
        className="h-9 rounded-lg px-3.5 text-base text-fg-muted hover:bg-hover"
      >
        Cancel
      </button>
      <button
        type="submit"
        disabled={disabled}
        className={cn(
          "h-9 rounded-lg px-3.5 text-base font-medium transition-opacity hover:opacity-90 disabled:opacity-40",
          danger ? "bg-danger text-app" : "bg-fg text-app",
        )}
      >
        {submitLabel}
      </button>
    </div>
  );
}

function ErrorLine({ error }: { error: unknown }) {
  if (!error) return null;
  return (
    <p role="alert" className="text-base text-danger">
      {errorMessage(error)}
    </p>
  );
}

/** Name and description. */
export function EditProjectDialog({
  project,
  open,
  onOpenChange,
}: {
  project: Project;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  return (
    <Modal open={open} onOpenChange={onOpenChange} title="Edit project details">
      <EditProjectForm project={project} onDone={() => onOpenChange(false)} />
    </Modal>
  );
}

function EditProjectForm({ project, onDone }: { project: Project; onDone: () => void }) {
  const update = useUpdateProject();
  const [name, setName] = useState(project.name);
  const [description, setDescription] = useState(project.description);
  return (
    <form
      className="flex flex-col gap-4"
      onSubmit={(e) => {
        e.preventDefault();
        update.mutate(
          { id: project.id, patch: patchOf({ name, description }) },
          { onSuccess: onDone },
        );
      }}
    >
      <label className="flex flex-col gap-1.5 text-sm font-medium text-fg-muted">
        Name
        <input
          autoFocus
          value={name}
          onChange={(e) => setName(e.target.value)}
          className={`${field} h-10`}
        />
      </label>
      <label className="flex flex-col gap-1.5 text-sm font-medium text-fg-muted">
        Description
        <textarea
          value={description}
          onChange={(e) => setDescription(e.target.value)}
          rows={3}
          className={`${field} resize-none py-2`}
        />
      </label>
      <ErrorLine error={update.error} />
      <DialogButtons
        onCancel={onDone}
        submitLabel="Save"
        disabled={!name.trim() || update.isPending}
      />
    </form>
  );
}

/** The project's instructions: what every chat in it should follow. */
export function InstructionsDialog({
  project,
  open,
  onOpenChange,
}: {
  project: Project;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  return (
    <Modal
      open={open}
      onOpenChange={onOpenChange}
      wide
      title="Set project instructions"
      description="Every chat in this project follows these, for example a role, a tone, or what to focus on."
    >
      <InstructionsForm project={project} onDone={() => onOpenChange(false)} />
    </Modal>
  );
}

function InstructionsForm({ project, onDone }: { project: Project; onDone: () => void }) {
  const update = useUpdateProject();
  const [text, setText] = useState(project.instructions);
  const over = text.length > MAX_INSTRUCTIONS;
  return (
    <form
      className="flex min-h-0 flex-col gap-4"
      onSubmit={(e) => {
        e.preventDefault();
        update.mutate(
          { id: project.id, patch: patchOf({ instructions: text }) },
          { onSuccess: onDone },
        );
      }}
    >
      <textarea
        autoFocus
        aria-label="Project instructions"
        value={text}
        onChange={(e) => setText(e.target.value)}
        rows={14}
        placeholder="e.g. Answer as a senior Rust engineer. Keep replies short and show code first."
        className={`${field} min-h-40 resize-y py-2 font-sans`}
      />
      <p className={cn("-mt-2 text-sm", over ? "text-danger" : "text-fg-subtle")}>
        {text.length.toLocaleString()} / {MAX_INSTRUCTIONS.toLocaleString()} characters
      </p>
      <ErrorLine error={update.error} />
      <DialogButtons
        onCancel={onDone}
        submitLabel="Save instructions"
        disabled={over || update.isPending}
      />
    </form>
  );
}

/** Text typed or pasted straight into the project's knowledge. */
export function AddTextDialog({
  projectId,
  open,
  onOpenChange,
}: {
  projectId: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  return (
    <Modal open={open} onOpenChange={onOpenChange} wide title="Add text content">
      <AddTextForm projectId={projectId} onDone={() => onOpenChange(false)} />
    </Modal>
  );
}

function AddTextForm({ projectId, onDone }: { projectId: string; onDone: () => void }) {
  const add = useAddProjectText(projectId);
  const [name, setName] = useState("");
  const [text, setText] = useState("");
  return (
    <form
      className="flex min-h-0 flex-col gap-4"
      onSubmit={(e) => {
        e.preventDefault();
        add.mutate({ name: name.trim() || "Untitled", text }, { onSuccess: onDone });
      }}
    >
      <label className="flex flex-col gap-1.5 text-sm font-medium text-fg-muted">
        Title
        <input
          autoFocus
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="e.g. Meeting notes"
          className={`${field} h-10`}
        />
      </label>
      <label className="flex flex-col gap-1.5 text-sm font-medium text-fg-muted">
        Content
        <textarea
          value={text}
          onChange={(e) => setText(e.target.value)}
          rows={12}
          className={`${field} min-h-40 resize-y py-2 font-mono text-base`}
        />
      </label>
      <ErrorLine error={add.error} />
      <DialogButtons
        onCancel={onDone}
        submitLabel="Add content"
        disabled={!text.trim() || add.isPending}
      />
    </form>
  );
}

/** Shows a knowledge file's stored text. */
export function KnowledgeFileDialog({
  fileId,
  onClose,
}: {
  fileId: string | null;
  onClose: () => void;
}) {
  const { data, error, isPending } = useKnowledgeFile(fileId);
  const title = data?.file.name ?? "File";
  return (
    <Modal
      open={fileId !== null}
      onOpenChange={(o) => {
        if (!o) onClose();
      }}
      wide
      title={title}
      description={
        data
          ? `${formatTokens(data.file.tokens)} tokens · ${data.file.chars.toLocaleString()} characters${
              data.file.pages ? ` · ${data.file.pages} pages` : ""
            }`
          : undefined
      }
    >
      {isPending && <p className="text-fg-muted">Loading…</p>}
      <ErrorLine error={error} />
      {data && (
        <pre className="min-h-0 flex-1 overflow-auto whitespace-pre-wrap rounded-lg border border-line bg-sunken p-3 font-mono text-sm text-fg">
          {data.text}
        </pre>
      )}
    </Modal>
  );
}

/** "Are you sure?" with a red button. */
export function ConfirmDialog({
  open,
  onOpenChange,
  title,
  body,
  confirmLabel,
  onConfirm,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  body: string;
  confirmLabel: string;
  onConfirm: () => void;
}) {
  return (
    <Modal open={open} onOpenChange={onOpenChange} title={title}>
      <form
        className="flex flex-col gap-5"
        onSubmit={(e) => {
          e.preventDefault();
          onConfirm();
          onOpenChange(false);
        }}
      >
        <p className="text-base text-fg-muted">{body}</p>
        <DialogButtons onCancel={() => onOpenChange(false)} submitLabel={confirmLabel} danger />
      </form>
    </Modal>
  );
}
