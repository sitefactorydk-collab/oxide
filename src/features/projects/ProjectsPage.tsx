import { Archive, FolderClosed, Plus } from "lucide-react";
import { useMemo, useState } from "react";
import { Link } from "react-router";
import { PageHeader, PrimaryButton, SearchField } from "@/components/PageHeader";
import { useConversations } from "@/features/chats/queries";
import { formatRelative } from "@/lib/time";
import { NewProjectDialog } from "./NewProjectDialog";
import { useProjects } from "./queries";

export function ProjectsPage() {
  const { data: projects, isPending } = useProjects();
  const { data: conversations } = useConversations();
  const [q, setQ] = useState("");
  const [creating, setCreating] = useState(false);
  const [showArchived, setShowArchived] = useState(false);
  const archivedCount = projects.filter((p) => p.archived).length;

  const list = useMemo(() => {
    const needle = q.trim().toLowerCase();
    return projects.filter(
      (p) =>
        p.archived === showArchived &&
        (!needle || `${p.name} ${p.description}`.toLowerCase().includes(needle)),
    );
  }, [projects, q, showArchived]);

  return (
    <div className="min-h-0 flex-1 overflow-y-auto">
      <div className="mx-auto w-full max-w-page px-6 py-10">
        <PageHeader
          title="Projects"
          action={
            <PrimaryButton onClick={() => setCreating(true)}>
              <Plus />
              New project
            </PrimaryButton>
          }
        />
        <SearchField
          value={q}
          onChange={setQ}
          label="Filter projects"
          placeholder="Filter projects…"
        />
        {(archivedCount > 0 || showArchived) && (
          <div className="mb-3 flex justify-end">
            <button
              type="button"
              aria-pressed={showArchived}
              onClick={() => setShowArchived((v) => !v)}
              className="inline-flex h-8 items-center gap-2 rounded-lg border border-line px-3 text-sm text-fg-muted hover:bg-hover hover:text-fg"
            >
              <Archive className="size-4" />
              {showArchived ? "Show active projects" : `Archived (${archivedCount})`}
            </button>
          </div>
        )}
        <ul className="grid grid-cols-1 gap-3 md:grid-cols-2">
          {list.map((p) => {
            const chats = conversations.filter((c) => c.projectId === p.id).length;
            return (
              <li key={p.id}>
                <Link
                  to={`/project/${p.id}`}
                  className="flex h-full flex-col gap-2 rounded-xl border border-line bg-raised p-4 shadow-sm transition-colors hover:border-line-strong"
                >
                  <span className="flex items-center gap-2 text-md font-medium text-fg">
                    <FolderClosed className="size-4 text-fg-muted" />
                    {p.name}
                    {p.archived && (
                      <span className="rounded-md bg-sunken px-1.5 py-0.5 text-xs font-normal text-fg-subtle">
                        Archived
                      </span>
                    )}
                  </span>
                  <span className="line-clamp-2 flex-1 text-base text-fg-muted">
                    {p.description || "No description"}
                  </span>
                  <span className="text-sm text-fg-subtle">
                    Updated {formatRelative(p.updatedAt)} · {chats} {chats === 1 ? "chat" : "chats"}
                  </span>
                </Link>
              </li>
            );
          })}
        </ul>
        {!isPending && showArchived && list.length === 0 && (
          <p className="text-center text-fg-muted">No archived projects.</p>
        )}
        {!isPending && projects.length === 0 && (
          <div className="rounded-xl border border-dashed border-line-strong p-8 text-center">
            <p className="text-md text-fg">No projects yet</p>
            <p className="mt-1 text-base text-fg-muted">
              Projects keep related chats together under shared instructions.
            </p>
          </div>
        )}
      </div>
      <NewProjectDialog open={creating} onOpenChange={setCreating} />
    </div>
  );
}
