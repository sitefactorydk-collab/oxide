import { X } from "lucide-react";
import { Dialog } from "radix-ui";
import { useState } from "react";
import { useNavigate } from "react-router";
import { IconButton } from "@/components/ui/IconButton";
import { errorMessage } from "@/lib/api";
import { useCreateProject } from "./queries";

/** Name + description, then opens the new project. */
export function NewProjectDialog({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-40 bg-overlay" />
        <Dialog.Content
          aria-describedby={undefined}
          className="ox-pop fixed left-1/2 top-1/2 z-50 w-[min(520px,calc(100vw-48px))] -translate-x-1/2 -translate-y-1/2 rounded-2xl border border-line bg-app p-6 shadow-lg outline-none"
        >
          <div className="mb-5 flex items-center justify-between">
            <Dialog.Title className="font-serif text-xl font-normal text-fg">
              New project
            </Dialog.Title>
            <Dialog.Close asChild>
              <IconButton size="sm" label="Close">
                <X className="size-4" />
              </IconButton>
            </Dialog.Close>
          </div>
          {open && <NewProjectForm onDone={() => onOpenChange(false)} />}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

function NewProjectForm({ onDone }: { onDone: () => void }) {
  const create = useCreateProject();
  const navigate = useNavigate();
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const field =
    "w-full rounded-lg border border-line bg-raised px-3 text-md text-fg outline-none transition-colors focus:border-accent";

  return (
    <form
      onSubmit={(e) => {
        e.preventDefault();
        create.mutate(
          { name, description },
          {
            onSuccess: (p) => {
              onDone();
              navigate(`/project/${p.id}`);
            },
          },
        );
      }}
      className="flex flex-col gap-4"
    >
      <label className="flex flex-col gap-1.5 text-sm font-medium text-fg-muted">
        Name
        <input
          autoFocus
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="e.g. Garden planner"
          className={`${field} h-10`}
        />
      </label>
      <label className="flex flex-col gap-1.5 text-sm font-medium text-fg-muted">
        What is it about? (optional)
        <textarea
          value={description}
          onChange={(e) => setDescription(e.target.value)}
          rows={3}
          className={`${field} resize-none py-2`}
        />
      </label>
      {create.error && (
        <p role="alert" className="text-base text-danger">
          {errorMessage(create.error)}
        </p>
      )}
      <div className="flex justify-end gap-2">
        <button
          type="button"
          onClick={onDone}
          className="h-9 rounded-lg px-3.5 text-base text-fg-muted hover:bg-hover"
        >
          Cancel
        </button>
        <button
          type="submit"
          disabled={!name.trim() || create.isPending}
          className="h-9 rounded-lg bg-fg px-3.5 text-base font-medium text-app transition-opacity hover:opacity-90 disabled:opacity-40"
        >
          Create project
        </button>
      </div>
    </form>
  );
}
