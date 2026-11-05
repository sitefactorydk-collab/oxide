import { Plus, Star } from "lucide-react";
import { useMemo, useState } from "react";
import { Link, useNavigate } from "react-router";
import { PageHeader, PrimaryButton, SearchField } from "@/components/PageHeader";
import { modelLabel } from "@/features/mock/data";
import { useMock } from "@/features/mock/store";
import { formatRelative } from "@/lib/time";

export function ChatsPage() {
  const conversations = useMock((s) => s.conversations);
  const projects = useMock((s) => s.projects);
  const navigate = useNavigate();
  const [q, setQ] = useState("");

  const list = useMemo(() => {
    const needle = q.trim().toLowerCase();
    return [...conversations]
      .filter((c) => !needle || c.title.toLowerCase().includes(needle))
      .sort((a, b) => b.updatedAt - a.updatedAt);
  }, [conversations, q]);

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
          placeholder="Filter your chats…"
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
                    {formatRelative(c.updatedAt)} · {modelLabel(c.modelId)}
                    {project ? ` · ${project.name}` : ""}
                  </span>
                </Link>
              </li>
            );
          })}
          {list.length === 0 && (
            <li className="py-8 text-center text-fg-muted">No matching chats</li>
          )}
        </ul>
      </div>
    </div>
  );
}
