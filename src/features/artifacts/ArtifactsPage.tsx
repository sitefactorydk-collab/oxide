import { Shapes, Trash2 } from "lucide-react";
import { useMemo, useState } from "react";
import { useNavigate } from "react-router";
import type { ArtifactKind, ArtifactSummary } from "@/bindings";
import { PageHeader, SearchField } from "@/components/PageHeader";
import { IconButton } from "@/components/ui/IconButton";
import { ConfirmDialog } from "@/features/projects/ProjectDialogs";
import { errorMessage } from "@/lib/api";
import { cn } from "@/lib/cn";
import { formatRelative } from "@/lib/time";
import { KindIcon } from "./KindIcon";
import { KIND_LABELS, kindLine } from "./kinds";
import { useArtifacts, useDeleteArtifact } from "./queries";
import { useArtifactPanel } from "./store";

const FILTERS: Array<{ id: ArtifactKind | "all"; label: string }> = [
  { id: "all", label: "All" },
  { id: "markdown", label: "Documents" },
  { id: "code", label: "Code" },
  { id: "html", label: "Web pages" },
  { id: "react", label: "Apps" },
  { id: "svg", label: "Images" },
  { id: "mermaid", label: "Diagrams" },
];

/** Every artifact across chats. Opening one goes to its chat with the panel open. */
export function ArtifactsPage() {
  const { data, isPending, error } = useArtifacts();
  const [q, setQ] = useState("");
  const [kind, setKind] = useState<ArtifactKind | "all">("all");
  const [deleting, setDeleting] = useState<ArtifactSummary | null>(null);
  const remove = useDeleteArtifact();
  const navigate = useNavigate();
  const open = useArtifactPanel((s) => s.open);

  const all = useMemo(() => data ?? [], [data]);
  const list = useMemo(() => {
    const needle = q.trim().toLowerCase();
    return all.filter(
      (r) =>
        (kind === "all" || r.artifact.kind === kind) &&
        (!needle ||
          `${r.artifact.title} ${r.conversationTitle} ${r.excerpt}`.toLowerCase().includes(needle)),
    );
  }, [all, q, kind]);
  const kinds = new Set(all.map((r) => r.artifact.kind));

  const show = (r: ArtifactSummary) => {
    open(r.artifact.conversationId, r.artifact.id);
    navigate(`/chat/${r.artifact.conversationId}`);
  };

  return (
    <div className="min-h-0 flex-1 overflow-y-auto">
      <div className="mx-auto w-full max-w-page px-6 py-10">
        <PageHeader title="Artifacts" />
        {all.length > 0 && (
          <>
            <SearchField
              value={q}
              onChange={setQ}
              label="Filter artifacts"
              placeholder="Filter artifacts…"
            />
            <div role="group" aria-label="Type" className="mb-4 flex flex-wrap gap-1.5">
              {FILTERS.filter((f) => f.id === "all" || kinds.has(f.id)).map((f) => (
                <button
                  key={f.id}
                  type="button"
                  aria-pressed={kind === f.id}
                  onClick={() => setKind(f.id)}
                  className={cn(
                    "h-8 rounded-full border px-3 text-sm",
                    kind === f.id
                      ? "border-fg bg-fg text-app"
                      : "border-line text-fg-muted hover:bg-hover hover:text-fg",
                  )}
                >
                  {f.label}
                </button>
              ))}
            </div>
          </>
        )}
        {error && (
          <p role="alert" className="text-danger">
            Couldn’t load artifacts: {errorMessage(error)}
          </p>
        )}
        {!isPending && all.length === 0 && !error && (
          <div className="flex flex-col items-center gap-3 rounded-xl border border-dashed border-line-strong px-6 py-14 text-center">
            <Shapes className="size-8 text-fg-subtle" strokeWidth={1.5} />
            <p className="text-md text-fg">No artifacts yet</p>
            <p className="max-w-sm text-base text-fg-muted">
              Documents, code, diagrams and small apps made in your chats collect here. Ask for one
              in any chat, for example “Make a to-do app as a React artifact”.
            </p>
          </div>
        )}
        {all.length > 0 && list.length === 0 && (
          <p className="text-fg-muted">No artifacts match.</p>
        )}
        <ul aria-label="Artifacts" className="grid grid-cols-1 gap-3 md:grid-cols-2">
          {list.map((r) => (
            <li key={r.artifact.id} className="group relative">
              <button
                type="button"
                onClick={() => show(r)}
                className="flex h-full w-full flex-col gap-2 rounded-xl border border-line bg-raised p-4 text-left shadow-sm transition-colors hover:border-line-strong"
              >
                <span className="flex items-center gap-2 pr-8 text-md font-medium text-fg">
                  <span className="text-fg-muted">
                    <KindIcon kind={r.artifact.kind} />
                  </span>
                  <span className="truncate">{r.artifact.title}</span>
                </span>
                <span className="line-clamp-2 flex-1 font-mono text-xs text-fg-muted">
                  {r.excerpt}
                </span>
                <span className="truncate text-sm text-fg-subtle">
                  {kindLine(r.artifact.kind, r.artifact.language)} ·{" "}
                  {r.artifact.version > 1 ? `${r.artifact.version} versions · ` : ""}
                  {formatRelative(r.artifact.updatedAt)} · {r.conversationTitle}
                </span>
              </button>
              <span className="absolute right-2 top-2 opacity-0 transition-opacity focus-within:opacity-100 group-hover:opacity-100">
                <IconButton
                  size="sm"
                  label={`Delete ${r.artifact.title}`}
                  onClick={() => setDeleting(r)}
                >
                  <Trash2 className="size-4" />
                </IconButton>
              </span>
            </li>
          ))}
        </ul>
        {remove.error && (
          <p role="alert" className="mt-3 text-danger">
            {errorMessage(remove.error)}
          </p>
        )}
      </div>
      <ConfirmDialog
        open={deleting !== null}
        onOpenChange={(o) => {
          if (!o) setDeleting(null);
        }}
        title="Delete artifact?"
        body={
          deleting
            ? `“${deleting.artifact.title}” (${KIND_LABELS[deleting.artifact.kind].toLowerCase()}) and all its versions will be deleted. The chat keeps its messages.`
            : ""
        }
        confirmLabel="Delete"
        onConfirm={() => {
          if (deleting) remove.mutate(deleting.artifact.id);
        }}
      />
    </div>
  );
}
