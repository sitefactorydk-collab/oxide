import { FileCode, FilePlus, Folder, Pencil, Save, Trash2, TriangleAlert } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import type { AppDraft } from "@/bindings";
import { IconButton } from "@/components/ui/IconButton";
import { errorMessage } from "@/lib/api";
import { cn } from "@/lib/cn";
import { toast } from "@/lib/toast";
import { CodeEditor } from "./CodeEditor";
import { fileTree } from "./fileTree";
import { useDeleteDraftFile, useRenameDraftFile, useSaveDraftFile } from "./queries";

const MANIFEST = "oxide-app.json";

/** A file being edited: what the user typed, and the saved text it started from. */
interface Buffer {
  content: string;
  base: string;
}

/** Code: the draft's files in a tree and an editor; saving makes a checkpoint. */
export function CodePane({ draft }: { draft: AppDraft }) {
  const id = draft.app.manifest.id;
  const entry = draft.app.manifest.entry ?? "App.tsx";
  const save = useSaveDraftFile(id);
  const remove = useDeleteDraftFile(id);
  const rename = useRenameDraftFile(id);
  const [selected, setSelected] = useState(entry);
  const [buffers, setBuffers] = useState<Record<string, Buffer>>({});
  const [naming, setNaming] = useState<null | { mode: "new" | "rename"; value: string }>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);

  const saved = useMemo(
    () => new Map(draft.files.map((f) => [f.path, f.content] as const)),
    [draft.files],
  );
  // Files only in a buffer are new files that aren't saved yet.
  const paths = useMemo(
    () => [...new Set([...saved.keys(), ...Object.keys(buffers)])],
    [saved, buffers],
  );
  const rows = useMemo(() => fileTree(paths), [paths]);

  // The selected file went away (the builder deleted or renamed it): show the entry.
  useEffect(() => {
    if (!paths.includes(selected)) setSelected(paths.includes(entry) ? entry : (paths[0] ?? ""));
  }, [paths, selected, entry]);

  const buffer = buffers[selected];
  const savedText = saved.get(selected);
  const value = buffer?.content ?? savedText ?? "";
  const dirty = (p: string) => {
    const b = buffers[p];
    return b !== undefined && b.content !== saved.get(p);
  };
  // The builder changed a file while it had unsaved edits here.
  const conflict = buffer !== undefined && savedText !== undefined && buffer.base !== savedText;

  const edit = (content: string) =>
    setBuffers((all) => ({
      ...all,
      [selected]: { content, base: all[selected]?.base ?? savedText ?? "" },
    }));

  const drop = (p: string) =>
    setBuffers((all) => {
      const next = { ...all };
      delete next[p];
      return next;
    });

  const saveCurrent = () => {
    if (!dirty(selected)) return;
    const path = selected;
    save.mutate(
      { path, content: value },
      {
        onSuccess: () => {
          drop(path);
          toast(`Saved ${path}.`);
        },
        onError: (e) => toast(errorMessage(e), "error"),
      },
    );
  };

  const finishNaming = () => {
    if (!naming) return;
    const name = naming.value.trim().replace(/^\.?\/+/, "");
    setNaming(null);
    if (!name || name === selected) return;
    if (paths.includes(name)) {
      toast(`${name} exists already.`, "error");
      return;
    }
    if (naming.mode === "new") {
      setBuffers((all) => ({ ...all, [name]: { content: "", base: "" } }));
      setSelected(name);
      return;
    }
    if (!saved.has(selected)) {
      // A new file that isn't saved yet: just rename its buffer.
      setBuffers((all) => {
        const next = { ...all, [name]: all[selected] ?? { content: "", base: "" } };
        delete next[selected];
        return next;
      });
      setSelected(name);
      return;
    }
    const from = selected;
    rename.mutate(
      { from, to: name },
      {
        onSuccess: () => {
          setBuffers((all) => {
            if (!all[from]) return all;
            const next = { ...all, [name]: all[from] };
            delete next[from];
            return next;
          });
          setSelected(name);
        },
        onError: (e) => toast(errorMessage(e), "error"),
      },
    );
  };

  const deleteCurrent = () => {
    if (!confirmDelete) {
      setConfirmDelete(true);
      return;
    }
    setConfirmDelete(false);
    const path = selected;
    if (!saved.has(path)) {
      drop(path);
      return;
    }
    remove.mutate(path, {
      onSuccess: () => drop(path),
      onError: (e) => toast(errorMessage(e), "error"),
    });
  };

  const fixed = selected === MANIFEST || selected === entry;
  const unsavedCount = paths.filter(dirty).length;

  return (
    <div className="flex min-h-0 flex-1">
      <nav
        aria-label="Files"
        className="flex w-56 shrink-0 flex-col border-r border-line bg-app text-sm"
      >
        <div className="flex h-9 items-center gap-1 border-b border-line px-2">
          <span className="flex-1 px-1 text-xs font-medium text-fg-subtle">Files</span>
          <IconButton
            size="sm"
            label="New file"
            onClick={() => setNaming({ mode: "new", value: "components/" })}
          >
            <FilePlus className="size-4" />
          </IconButton>
        </div>
        {naming && (
          <form
            className="border-b border-line p-2"
            onSubmit={(e) => {
              e.preventDefault();
              finishNaming();
            }}
          >
            <input
              autoFocus
              aria-label={naming.mode === "new" ? "New file name" : "New name"}
              value={naming.value}
              onChange={(e) => setNaming({ ...naming, value: e.target.value })}
              onBlur={finishNaming}
              onKeyDown={(e) => e.key === "Escape" && setNaming(null)}
              className="h-7 w-full rounded-md border border-line bg-raised px-2 text-sm text-fg outline-none focus:border-accent"
            />
          </form>
        )}
        <ul className="min-h-0 flex-1 overflow-y-auto py-1">
          {rows.map((r) => (
            <li key={r.path}>
              {r.folder ? (
                <div
                  className="flex h-7 items-center gap-1.5 text-fg-muted"
                  style={{ paddingLeft: 8 + r.depth * 12 }}
                >
                  <Folder className="size-3.5 shrink-0" />
                  <span className="truncate">{r.name}</span>
                </div>
              ) : (
                <button
                  type="button"
                  aria-current={r.path === selected ? "true" : undefined}
                  onClick={() => {
                    setSelected(r.path);
                    setConfirmDelete(false);
                  }}
                  className={cn(
                    "flex h-7 w-full items-center gap-1.5 pr-2 text-left hover:bg-hover",
                    r.path === selected ? "bg-active text-fg" : "text-fg-muted",
                  )}
                  style={{ paddingLeft: 8 + r.depth * 12 }}
                >
                  <FileCode className="size-3.5 shrink-0" />
                  <span className="min-w-0 flex-1 truncate">{r.name}</span>
                  {dirty(r.path) && (
                    <span
                      aria-label="Unsaved"
                      className="size-1.5 shrink-0 rounded-full bg-accent"
                    />
                  )}
                </button>
              )}
            </li>
          ))}
        </ul>
      </nav>
      <div className="flex min-w-0 flex-1 flex-col">
        <div className="flex h-9 shrink-0 items-center gap-1 border-b border-line px-3">
          <span className="min-w-0 flex-1 truncate font-mono text-xs text-fg-muted">
            {selected}
          </span>
          {!fixed && selected && (
            <>
              <IconButton
                size="sm"
                label="Rename file"
                onClick={() => setNaming({ mode: "rename", value: selected })}
              >
                <Pencil className="size-3.5" />
              </IconButton>
              <IconButton
                size="sm"
                label={confirmDelete ? "Click again to delete" : "Delete file"}
                onClick={deleteCurrent}
                onBlur={() => setConfirmDelete(false)}
                className={confirmDelete ? "text-danger" : undefined}
              >
                <Trash2 className="size-3.5" />
              </IconButton>
            </>
          )}
          <button
            type="button"
            onClick={saveCurrent}
            disabled={!dirty(selected) || save.isPending}
            title="Save (Ctrl+S)"
            className="ml-1 inline-flex h-7 items-center gap-1.5 rounded-md bg-fg px-2.5 text-xs font-medium text-app hover:opacity-90 disabled:opacity-30"
          >
            <Save className="size-3.5" />
            Save{unsavedCount > 1 ? ` (${unsavedCount} unsaved)` : ""}
          </button>
        </div>
        {conflict && (
          <div className="flex items-center gap-2 border-b border-line bg-sunken px-3 py-1.5 text-xs text-fg-muted">
            <TriangleAlert className="size-3.5 shrink-0 text-danger" />
            <span className="flex-1">The builder changed this file while you were editing it.</span>
            <button
              type="button"
              className="text-accent hover:underline"
              onClick={() => drop(selected)}
            >
              Use the builder's version
            </button>
          </div>
        )}
        {selected ? (
          <CodeEditor
            key={selected}
            path={selected}
            value={value}
            onChange={edit}
            onSave={saveCurrent}
          />
        ) : (
          <p className="p-6 text-sm text-fg-muted">No file selected.</p>
        )}
      </div>
    </div>
  );
}
