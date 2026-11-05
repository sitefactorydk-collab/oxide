import { Plus, Star } from "lucide-react";
import { useMemo, useState } from "react";
import { Link, useNavigate } from "react-router";
import { PageHeader, PrimaryButton, SearchField } from "@/components/PageHeader";
import type { Conversation } from "@/bindings";
import { useConversations, useSearch } from "@/features/chats/queries";
import { findModel, modelNameFromId } from "@/features/models/models";
import { useModels } from "@/features/models/queries";
import { useProjects } from "@/features/projects/queries";
import { formatRelative } from "@/lib/time";
import { useDebounced } from "@/lib/useDebounced";

export function ChatsPage() {
  const { data: conversations } = useConversations();
  const { data: projects } = useProjects();
  const { data: models } = useModels();
  const navigate = useNavigate();
  const [q, setQ] = useState("");
  const query = useDebounced(q.trim(), 150);
  const search = useSearch(query, 200);

  // Filtering uses full-text search over titles and message text.
  const list = useMemo<Conversation[]>(
    () => (query ? (search.data ?? []).map((h) => h.conversation) : conversations),
    [query, search.data, conversations],
  );

  return (
    <div className="min-h-0 flex-1 overflow-y-auto">
      <div className="mx-auto w-full max-w-thread px-6 py-10">
        <PageHeader
          title="Chats"
          action={
            <PrimaryButton onClick={() => navigate("/new")}>
              <Plus />
              New chat
            </PrimaryButton>
          }
        />
        <SearchField
          value={q}
          onChange={setQ}
          label="Filter chats"
          placeholder="Search your chats…"
        />
        <p className="mb-2 text-sm text-fg-subtle">
          {list.length} {list.length === 1 ? "chat" : "chats"}
        </p>
        <ul className="divide-y divide-line border-y border-line">
          {list.map((c) => {
            const project = projects.find((p) => p.id === c.projectId);
            return (
              <li key={c.id}>
                <Link
                  to={`/chat/${c.id}`}
                  className="-mx-3 flex flex-col gap-0.5 rounded-lg px-3 py-3 transition-colors hover:bg-hover"
                >
                  <span className="flex items-center gap-2 text-md text-fg">
                    {c.starred && <Star className="size-3.5 fill-current text-accent" />}
                    {c.title}
                  </span>
                  <span className="text-sm text-fg-subtle">
                    {formatRelative(c.updatedAt)} ·{" "}
                    {findModel(models, c.model)?.label ?? modelNameFromId(c.model)}
                    {project ? ` · ${project.name}` : ""}
                  </span>
                </Link>
              </li>
            );
          })}
          {list.length === 0 && (
            <li className="py-8 text-center text-fg-muted">
              {query ? "No matching chats" : "No chats yet"}
            </li>
          )}
        </ul>
      </div>
    </div>
  );
}
