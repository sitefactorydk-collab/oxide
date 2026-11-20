import { Dialog } from "radix-ui";
import { Download, LayoutGrid, Lock, Plus, TriangleAlert, X } from "lucide-react";
import { useMemo, useState } from "react";
import { Link, useNavigate } from "react-router";
import type { AppImportPreview, UtilityApp } from "@/bindings";
import { PageHeader, PrimaryButton, SearchField } from "@/components/PageHeader";
import { IconButton } from "@/components/ui/IconButton";
import { errorMessage } from "@/lib/api";
import { cn } from "@/lib/cn";
import { formatBytes } from "@/lib/time";
import { toast } from "@/lib/toast";
import { useAppDrafts, useOpenBuilder } from "@/features/builder/queries";
import { useNewChatModel } from "@/features/models/queries";
import { AppIcon } from "./AppIcon";
import { CAPABILITY_TEXT, useApps, useInstallImport, usePickImport } from "./queries";

/** The Utility Apps gallery (`/apps`). */
export function AppsPage() {
  const { data: apps, isPending, error } = useApps();
  const [q, setQ] = useState("");
  const [category, setCategory] = useState<string | null>(null);
  const pick = usePickImport();
  const { data: drafts } = useAppDrafts();
  const openBuilder = useOpenBuilder();
  const [model] = useNewChatModel();
  const navigate = useNavigate();
  const [preview, setPreview] = useState<AppImportPreview | null>(null);

  const categories = useMemo(
    () => [...new Set((apps ?? []).map((a) => a.manifest.category ?? "Other"))].sort(),
    [apps],
  );
  const list = useMemo(() => {
    const needle = q.trim().toLowerCase();
    return (apps ?? []).filter(
      (a) =>
        (!category || (a.manifest.category ?? "Other") === category) &&
        (!needle ||
          `${a.manifest.name} ${a.manifest.description ?? ""} ${a.manifest.category ?? ""}`
            .toLowerCase()
            .includes(needle)),
    );
  }, [apps, q, category]);
  const mine = list.filter((a) => !a.builtin);
  const builtIn = list.filter((a) => a.builtin);

  const newApp = () => {
    if (!model) {
      toast("Add a chat model in Settings → Providers first: the builder needs one.", "error");
      return;
    }
    openBuilder.mutate(
      { id: null, model },
      {
        onSuccess: (d) => navigate(`/apps/${d.app.manifest.id}/build`),
        onError: (e) => toast(errorMessage(e), "error"),
      },
    );
  };

  const startImport = () =>
    pick.mutate(undefined, {
      onSuccess: (p) => p && setPreview(p),
      onError: (e) => toast(errorMessage(e), "error"),
    });

  return (
    <div className="min-h-0 flex-1 overflow-y-auto">
      <div className="mx-auto w-full max-w-page px-6 py-10">
        <PageHeader
          title="Utility Apps"
          action={
            <div className="flex items-center gap-2">
              <button
                type="button"
                onClick={startImport}
                disabled={pick.isPending}
                className="inline-flex h-9 items-center gap-1.5 rounded-lg border border-line px-3.5 text-base text-fg hover:bg-hover disabled:opacity-40 [&_svg]:size-4"
              >
                <Download />
                Import app
              </button>
              <PrimaryButton onClick={newApp} disabled={openBuilder.isPending}>
                <Plus />
                New app
              </PrimaryButton>
            </div>
          }
        />
        <SearchField value={q} onChange={setQ} label="Search apps" placeholder="Search apps…" />
        {categories.length > 1 && (
          <div className="mb-5 flex flex-wrap gap-2" role="group" aria-label="Categories">
            {[null, ...categories].map((c) => (
              <button
                key={c ?? "all"}
                type="button"
                aria-pressed={category === c}
                onClick={() => setCategory(c)}
                className={cn(
                  "h-8 rounded-full border px-3 text-sm transition-colors",
                  category === c
                    ? "border-fg bg-fg text-app"
                    : "border-line text-fg-muted hover:bg-hover hover:text-fg",
                )}
              >
                {c ?? "All"}
              </button>
            ))}
          </div>
        )}
        {error && <p className="text-danger">{errorMessage(error)}</p>}
        {(drafts ?? []).length > 0 && !q && !category && (
          <AppSection title="Drafts" apps={drafts ?? []} draft />
        )}
        {mine.length > 0 && <AppSection title="My apps" apps={mine} />}
        {builtIn.length > 0 && <AppSection title="Built-in" apps={builtIn} />}
        {!isPending && list.length === 0 && (
          <div className="rounded-xl border border-dashed border-line-strong p-8 text-center">
            <LayoutGrid className="mx-auto mb-2 size-6 text-fg-subtle" />
            <p className="text-md text-fg">No apps match</p>
          </div>
        )}
      </div>
      <ImportDialog preview={preview} onClose={() => setPreview(null)} />
    </div>
  );
}

function AppSection({
  title,
  apps,
  draft = false,
}: {
  title: string;
  apps: UtilityApp[];
  draft?: boolean;
}) {
  return (
    <section className="mb-8" aria-label={title}>
      <h2 className="mb-3 text-sm font-medium text-fg-muted">{title}</h2>
      <ul className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">
        {apps.map((a) => (
          <li key={a.manifest.id}>
            <AppCard app={a} draft={draft} />
          </li>
        ))}
      </ul>
    </section>
  );
}

function AppCard({ app, draft }: { app: UtilityApp; draft: boolean }) {
  const m = app.manifest;
  return (
    <Link
      to={draft ? `/apps/${m.id}/build` : `/apps/${m.id}`}
      className="flex h-full gap-3 rounded-xl border border-line bg-raised p-4 shadow-sm transition-colors hover:border-line-strong"
    >
      <AppIcon icon={m.icon ?? "sparkles"} />
      <span className="flex min-w-0 flex-1 flex-col gap-1">
        <span className="flex items-center gap-2 text-md font-medium text-fg">
          <span className="truncate">{m.name}</span>
          {app.builtin && (
            <Lock className="size-3.5 shrink-0 text-fg-subtle" aria-label="Built-in" />
          )}
        </span>
        <span className="line-clamp-2 text-base text-fg-muted">
          {app.problem ? (
            <span className="inline-flex items-center gap-1 text-danger">
              <TriangleAlert className="size-3.5" /> {app.problem}
            </span>
          ) : (
            m.description || "No description"
          )}
        </span>
        <span className="mt-auto text-sm text-fg-subtle">
          {draft ? "Not saved yet · opens in the builder" : m.category}
        </span>
      </span>
    </Link>
  );
}

/** What an `.oxideapp` holds and asks for, before it is installed. */
function ImportDialog({
  preview,
  onClose,
}: {
  preview: AppImportPreview | null;
  onClose: () => void;
}) {
  const install = useInstallImport();
  const navigate = useNavigate();
  const m = preview?.manifest;
  return (
    <Dialog.Root open={Boolean(preview)} onOpenChange={(o) => !o && onClose()}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-40 bg-overlay" />
        <Dialog.Content
          aria-describedby={undefined}
          className="ox-pop fixed left-1/2 top-1/2 z-50 w-[min(520px,calc(100vw-48px))] -translate-x-1/2 -translate-y-1/2 rounded-2xl border border-line bg-app p-6 shadow-lg outline-none"
        >
          <div className="mb-4 flex items-center justify-between">
            <Dialog.Title className="font-serif text-xl font-normal text-fg">
              Import app
            </Dialog.Title>
            <Dialog.Close asChild>
              <IconButton tooltip={false} size="sm" label="Close">
                <X className="size-4" />
              </IconButton>
            </Dialog.Close>
          </div>
          {preview && m && (
            <div className="flex flex-col gap-4 text-base">
              <div className="flex gap-3">
                <AppIcon icon={m.icon ?? "sparkles"} />
                <div className="min-w-0">
                  <p className="font-medium text-fg">{m.name}</p>
                  <p className="text-fg-muted">{m.description || "No description"}</p>
                  <p className="mt-1 text-sm text-fg-subtle">
                    {m.category} · version {m.version} · {preview.files} files ·{" "}
                    {formatBytes(preview.bytes)}
                    {m.author ? ` · by ${m.author}` : ""}
                  </p>
                </div>
              </div>
              <div>
                <p className="mb-1 text-sm font-medium text-fg-muted">It may ask to</p>
                {(m.capabilities ?? []).length === 0 ? (
                  <p className="text-fg-muted">Nothing: it uses no models or tools.</p>
                ) : (
                  <ul className="list-disc pl-5 text-fg">
                    {(m.capabilities ?? []).map((c) => (
                      <li key={c}>{CAPABILITY_TEXT[c].ask}</li>
                    ))}
                  </ul>
                )}
                <p className="mt-1 text-sm text-fg-subtle">
                  Each one asks once, the first time the app uses it.
                </p>
              </div>
              {preview.replaces && (
                <p className="rounded-lg bg-sunken p-3 text-sm text-fg">
                  You already have an app with this id. Installing replaces it and forgets its
                  settings and permissions.
                </p>
              )}
              {preview.renamedFrom && (
                <p className="rounded-lg bg-sunken p-3 text-sm text-fg">
                  A built-in app has this id, so the import is installed as a copy.
                </p>
              )}
              <p className="text-sm text-fg-subtle">
                Apps run in a sandbox without network access; Oxide only gives them what you allow.
                Install apps from people you trust.
              </p>
              <div className="flex justify-end gap-2">
                <button
                  type="button"
                  onClick={onClose}
                  className="h-9 rounded-lg border border-line px-3.5 text-base text-fg hover:bg-hover"
                >
                  Cancel
                </button>
                <PrimaryButton
                  disabled={install.isPending}
                  onClick={() =>
                    install.mutate(preview.token, {
                      onSuccess: (app) => {
                        onClose();
                        toast(`Installed ${app.manifest.name}.`);
                        navigate(`/apps/${app.manifest.id}`);
                      },
                      onError: (e) => toast(errorMessage(e), "error"),
                    })
                  }
                >
                  Install
                </PrimaryButton>
              </div>
            </div>
          )}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
