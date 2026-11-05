import { FolderClosed, Plus } from "lucide-react";
import { useMemo, useState } from "react";
import { Link } from "react-router";
import { PageHeader, PrimaryButton, SearchField } from "@/components/PageHeader";
import { useMock } from "@/features/mock/store";
import { formatRelative } from "@/lib/time";

export function ProjectsPage() {
  const projects = useMock((s) => s.projects);
  const conversations = useMock((s) => s.conversations);
  const [q, setQ] = useState("");

  const list = useMemo(() => {
    const needle = q.trim().toLowerCase();
    return projects.filter(
      (p) => !needle || `${p.name} ${p.description}`.toLowerCase().includes(needle),
    );
  }, [projects, q]);

  return (
    <div className="min-h-0 flex-1 overflow-y-auto">
      <div className="mx-auto w-full max-w-page px-6 py-10">
        <PageHeader
          title="Projects"
          action={
            <PrimaryButton disabled title="Projects arrive in Phase 7">
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
                  </span>
                  <span className="line-clamp-2 flex-1 text-base text-fg-muted">
                    {p.description}
                  </span>
                  <span className="text-sm text-fg-subtle">
                    Updated {formatRelative(p.updatedAt)} · {chats} {chats === 1 ? "chat" : "chats"}
                  </span>
                </Link>
              </li>
            );
          })}
        </ul>
      </div>
    </div>
  );
}
