import { Dialog } from "radix-ui";
import { FileCode, Plus, X } from "lucide-react";
import { useEffect, useState } from "react";
import type { AppBundle } from "@/bindings";
import { IconButton } from "@/components/ui/IconButton";
import { errorMessage } from "@/lib/api";
import { cn } from "@/lib/cn";
import { toast } from "@/lib/toast";
import { useSaveAppFile } from "./queries";

/**
 * A plain editor for an own app's files (Phase 20). The builder with AI, Preview and a code
 * editor comes in Phase 21.
 */
export function CodeDialog({
  bundle,
  open,
  onOpenChange,
  onSaved,
}: {
  bundle: AppBundle;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSaved: () => void;
}) {
  const id = bundle.app.manifest.id;
  const save = useSaveAppFile();
  const [files, setFiles] = useState(bundle.files);
  const [path, setPath] = useState(bundle.app.manifest.entry ?? "App.tsx");
  const [dirty, setDirty] = useState<Set<string>>(new Set());
  useEffect(() => {
    if (open) {
      setFiles(bundle.files);
      setDirty(new Set());
    }
  }, [open, bundle.files]);
  const current = files.find((f) => f.path === path);

  const saveAll = async () => {
    try {
      for (const f of files.filter((f) => dirty.has(f.path))) {
        await save.mutateAsync({ id, path: f.path, content: f.content });
      }
      setDirty(new Set());
      onSaved();
      toast("Saved. The app reloads.");
    } catch (e) {
      toast(errorMessage(e), "error");
    }
  };

  const [newName, setNewName] = useState("");
  const addFile = () => {
    const name = newName.trim().replace(/^\/+/, "");
    if (!name || files.some((f) => f.path === name)) return;
    setNewName("");
    setFiles([...files, { path: name, content: "" }]);
    setDirty(new Set([...dirty, name]));
    setPath(name);
  };

  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-40 bg-overlay" />
        <Dialog.Content
          aria-describedby={undefined}
          className="ox-pop fixed inset-6 z-50 flex flex-col rounded-2xl border border-line bg-app shadow-lg outline-none"
        >
          <div className="flex items-center justify-between border-b border-line px-5 py-3">
            <Dialog.Title className="font-serif text-lg font-normal text-fg">
              Edit {bundle.app.manifest.name}
            </Dialog.Title>
            <div className="flex items-center gap-2">
              <button
                type="button"
                disabled={dirty.size === 0 || save.isPending}
                onClick={() => void saveAll()}
                className="h-8 rounded-lg bg-fg px-3 text-sm font-medium text-app hover:opacity-90 disabled:opacity-40"
              >
                Save{dirty.size > 0 ? ` (${dirty.size})` : ""}
              </button>
              <Dialog.Close asChild>
                <IconButton tooltip={false} size="sm" label="Close">
                  <X className="size-4" />
                </IconButton>
              </Dialog.Close>
            </div>
          </div>
          <div className="flex min-h-0 flex-1">
            <nav
              aria-label="Files"
              className="w-56 shrink-0 overflow-y-auto border-r border-line p-2"
            >
              {files.map((f) => (
                <button
                  key={f.path}
                  type="button"
                  onClick={() => setPath(f.path)}
                  className={cn(
                    "flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm",
                    f.path === path ? "bg-active text-fg" : "text-fg-muted hover:bg-hover",
                  )}
                >
                  <FileCode className="size-4 shrink-0" />
                  <span className="truncate">{f.path}</span>
                  {dirty.has(f.path) && <span className="ml-auto text-accent">•</span>}
                </button>
              ))}
              <form
                className="mt-2 flex items-center gap-1"
                onSubmit={(e) => {
                  e.preventDefault();
                  addFile();
                }}
              >
                <input
                  aria-label="New file"
                  value={newName}
                  onChange={(e) => setNewName(e.target.value)}
                  placeholder="lib/helpers.ts"
                  className="h-7 min-w-0 flex-1 rounded-md border border-line bg-raised px-2 text-sm text-fg outline-none focus:border-accent"
                />
                <IconButton size="sm" label="Add the file" type="submit">
                  <Plus className="size-4" />
                </IconButton>
              </form>
            </nav>
            <textarea
              aria-label={current?.path ?? "File"}
              spellCheck={false}
              value={current?.content ?? ""}
              onChange={(e) => {
                const content = e.target.value;
                setFiles(files.map((f) => (f.path === path ? { ...f, content } : f)));
                setDirty(new Set([...dirty, path]));
              }}
              className="min-h-0 flex-1 resize-none bg-code p-4 font-mono text-sm text-fg outline-none"
            />
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
