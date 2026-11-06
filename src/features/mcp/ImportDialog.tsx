import { X } from "lucide-react";
import { Dialog } from "radix-ui";
import { useEffect, useState } from "react";
import type { ImportStatus } from "@/bindings";
import { IconButton } from "@/components/ui/IconButton";
import { errorMessage } from "@/lib/api";
import { toast } from "@/lib/toast";
import { useImportMcp, useImportPreview } from "./queries";
import { Button } from "./ui";

const STATUS: Record<ImportStatus, string> = {
  new: "New",
  same: "Already in Oxide",
  changed: "Replaces Oxide's entry",
};

/** "Import from Claude desktop": shows what its config has, then copies the chosen entries. */
export function ImportDialog({ onClose }: { onClose: () => void }) {
  const preview = useImportPreview(true);
  const run = useImportMcp();
  const [chosen, setChosen] = useState<Set<string>>(new Set());
  const [error, setError] = useState<string | null>(null);

  // Preselect everything that would change something.
  useEffect(() => {
    if (preview.data) {
      setChosen(
        new Set(
          preview.data.candidates.filter((c) => c.status === "new").map((c) => c.config.name),
        ),
      );
    }
  }, [preview.data]);

  const toggle = (name: string) =>
    setChosen((prev) => {
      const next = new Set(prev);
      if (next.has(name)) next.delete(name);
      else next.add(name);
      return next;
    });

  const onImport = async () => {
    setError(null);
    try {
      const n = await run.mutateAsync([...chosen]);
      toast(`Imported ${n} server${n === 1 ? "" : "s"} from Claude desktop.`);
      onClose();
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  const data = preview.data;
  return (
    <Dialog.Root open onOpenChange={(o) => !o && onClose()}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-[60] bg-overlay" />
        <Dialog.Content className="ox-pop fixed left-1/2 top-1/2 z-[60] flex max-h-[min(560px,calc(100vh-64px))] w-[min(560px,calc(100vw-48px))] -translate-x-1/2 -translate-y-1/2 flex-col rounded-2xl border border-line bg-app p-6 shadow-lg outline-none">
          <div className="mb-1 flex items-start justify-between gap-4">
            <Dialog.Title className="font-serif text-xl font-normal text-fg">
              Import from Claude desktop
            </Dialog.Title>
            <Dialog.Close asChild>
              <IconButton label="Close">
                <X className="size-[18px]" />
              </IconButton>
            </Dialog.Close>
          </div>
          <Dialog.Description className="mb-4 text-base text-fg-muted">
            Oxide reads Claude desktop’s config without changing it, and copies the servers you pick
            into its own.
          </Dialog.Description>
          <div className="min-h-0 flex-1 overflow-y-auto">
            {preview.isPending ? (
              <p className="text-fg-muted">Reading…</p>
            ) : preview.error ? (
              <p className="text-danger">{errorMessage(preview.error)}</p>
            ) : !data?.found ? (
              <p className="text-fg-muted">
                No Claude desktop config was found. Oxide looked at{" "}
                <span className="break-all font-mono text-sm">{data?.path}</span>.
              </p>
            ) : data.candidates.length === 0 ? (
              <p className="text-fg-muted">
                Claude desktop’s config has no MCP servers (
                <span className="break-all font-mono text-sm">{data.path}</span>).
              </p>
            ) : (
              <>
                <p className="mb-2 break-all font-mono text-xs text-fg-subtle">{data.path}</p>
                <ul className="flex flex-col gap-1.5">
                  {data.candidates.map((c) => {
                    const name = c.config.name;
                    const detail =
                      c.config.transport === "http"
                        ? c.config.url
                        : [c.config.command, ...c.config.args].join(" ");
                    return (
                      <li key={name}>
                        <label className="flex cursor-pointer items-start gap-3 rounded-lg border border-line bg-raised px-3 py-2 hover:bg-hover">
                          <input
                            type="checkbox"
                            className="mt-1 accent-accent"
                            checked={chosen.has(name)}
                            disabled={c.status === "same"}
                            onChange={() => toggle(name)}
                          />
                          <span className="min-w-0 flex-1">
                            <span className="flex items-center gap-2">
                              <span className="font-medium text-fg">{name}</span>
                              <span className="text-xs text-fg-subtle">{STATUS[c.status]}</span>
                            </span>
                            <span className="block truncate font-mono text-xs text-fg-muted">
                              {detail}
                            </span>
                          </span>
                        </label>
                      </li>
                    );
                  })}
                </ul>
              </>
            )}
          </div>
          {error && (
            <p role="alert" className="mt-3 text-base text-danger">
              {error}
            </p>
          )}
          <div className="mt-5 flex justify-end gap-2">
            <Button onClick={onClose}>Cancel</Button>
            <Button
              primary
              disabled={chosen.size === 0 || run.isPending}
              onClick={() => void onImport()}
            >
              Import {chosen.size > 0 ? chosen.size : ""}
            </Button>
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
