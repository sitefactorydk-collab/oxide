import { Copy, Download, FileCode, RotateCw, Settings, Trash2 } from "lucide-react";
import { useState } from "react";
import { Link, useNavigate, useParams } from "react-router";
import { errorMessage } from "@/lib/api";
import { toast } from "@/lib/toast";
import { AppFrame } from "./AppFrame";
import { AppIcon } from "./AppIcon";
import { AppSettingsDialog } from "./AppSettingsDialog";
import { CodeDialog } from "./CodeDialog";
import { useAppBundle, useCloneApp, useDeleteApp, useExportApp } from "./queries";

const button =
  "inline-flex h-8 items-center gap-1.5 rounded-lg border border-line px-3 text-sm text-fg hover:bg-hover disabled:opacity-40 [&_svg]:size-4";

/** One Utility App, full page (`/apps/:id`), with a slim header. */
export function AppPage() {
  const { id } = useParams();
  const navigate = useNavigate();
  const { data: bundle, error, refetch, isPending } = useAppBundle(id);
  const clone = useCloneApp();
  const exportApp = useExportApp();
  const remove = useDeleteApp();
  const [reloadKey, setReloadKey] = useState(0);
  const [settings, setSettings] = useState(false);
  const [editing, setEditing] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);

  if (error) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center gap-3 p-6 text-center">
        <p className="text-md text-fg">{errorMessage(error)}</p>
        <Link to="/apps" className="text-accent hover:underline">
          Back to Utility Apps
        </Link>
      </div>
    );
  }
  if (isPending || !bundle) return <div className="flex-1" />;
  const app = bundle.app;
  const m = app.manifest;

  const reload = () => {
    void refetch().then(() => setReloadKey((k) => k + 1));
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <header className="flex items-center gap-3 border-b border-line px-4 py-2">
        <AppIcon icon={m.icon ?? "sparkles"} className="size-8" />
        <div className="min-w-0 flex-1">
          <h1 className="truncate text-md font-medium text-fg">{m.name}</h1>
          <p className="truncate text-sm text-fg-subtle">
            {app.builtin ? "Built-in" : "My app"} · {m.category} · v{m.version}
          </p>
        </div>
        <div role="toolbar" aria-label="App actions" className="flex items-center gap-2">
          <button type="button" className={button} onClick={reload} title="Reload the app">
            <RotateCw /> Reload
          </button>
          <button type="button" className={button} onClick={() => setSettings(true)}>
            <Settings /> Settings
          </button>
          <button
            type="button"
            className={button}
            disabled={clone.isPending}
            onClick={() =>
              clone.mutate(m.id, {
                onSuccess: (c) => {
                  toast(`Made “${c.manifest.name}” in My apps.`);
                  navigate(`/apps/${c.manifest.id}`);
                },
                onError: (e) => toast(errorMessage(e), "error"),
              })
            }
          >
            <Copy /> Clone
          </button>
          {!app.builtin && (
            <button type="button" className={button} onClick={() => setEditing(true)}>
              <FileCode /> Edit
            </button>
          )}
          <button
            type="button"
            className={button}
            disabled={exportApp.isPending}
            onClick={() =>
              exportApp.mutate(m.id, {
                onSuccess: (path) => path && toast(`Exported to ${path}.`),
                onError: (e) => toast(errorMessage(e), "error"),
              })
            }
          >
            <Download /> Export
          </button>
          {!app.builtin && (
            <button
              type="button"
              className={button}
              aria-label={confirmDelete ? "Click again to delete" : "Delete the app"}
              title="Deletes the app with its settings and storage"
              onBlur={() => setConfirmDelete(false)}
              onClick={() => {
                if (!confirmDelete) {
                  setConfirmDelete(true);
                  return;
                }
                remove.mutate(m.id, {
                  onSuccess: () => navigate("/apps"),
                  onError: (e) => toast(errorMessage(e), "error"),
                });
              }}
            >
              <Trash2 />
              {confirmDelete && <span className="text-danger">Delete?</span>}
            </button>
          )}
        </div>
      </header>
      <AppFrame bundle={bundle} reloadKey={reloadKey} />
      <AppSettingsDialog app={app} open={settings} onOpenChange={setSettings} />
      {!app.builtin && (
        <CodeDialog bundle={bundle} open={editing} onOpenChange={setEditing} onSaved={reload} />
      )}
    </div>
  );
}
