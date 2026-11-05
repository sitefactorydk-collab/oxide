import { FileText, Pencil, Plus } from "lucide-react";
import { useState, type ReactNode } from "react";
import { Link, useNavigate, useParams } from "react-router";
import { IconButton } from "@/components/ui/IconButton";
import { Composer } from "@/features/composer/Composer";
import { useMock } from "@/features/mock/store";
import { formatBytes, formatRelative } from "@/lib/time";
import { useUi } from "@/stores/ui";

export function ProjectPage() {
  const { id = "" } = useParams();
  const project = useMock((s) => s.projects.find((p) => p.id === id));
  const conversations = useMock((s) => s.conversations);
  const createConversation = useMock((s) => s.createConversation);
  const modelId = useUi((s) => s.modelId);
  const navigate = useNavigate();
  const [draft, setDraft] = useState("");

  if (!project) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center gap-3">
        <p className="font-serif text-2xl text-fg">Project not found</p>
        <Link to="/projects" className="text-accent hover:underline">
          Back to projects
        </Link>
      </div>
    );
  }

  const chats = conversations
    .filter((c) => c.projectId === project.id)
    .sort((a, b) => b.updatedAt - a.updatedAt);

  return (
    <div className="min-h-0 flex-1 overflow-y-auto">
      <div className="mx-auto grid w-full max-w-page grid-cols-1 gap-8 px-6 py-10 lg:grid-cols-[minmax(0,1fr)_18rem]">
        <div className="min-w-0">
          <h1 className="font-serif text-2xl font-normal tracking-tight text-fg">{project.name}</h1>
          <p className="mb-6 mt-1 text-md text-fg-muted">{project.description}</p>
          <Composer
            variant="home"
            value={draft}
            onChange={setDraft}
            placeholder={`Start a chat in ${project.name}…`}
            onSubmit={(text) => {
              const cid = createConversation(text || "Attached files", modelId, project.id);
              setDraft("");
              navigate(`/chat/${cid}`);
            }}
          />
          <h2 className="mb-2 mt-8 text-sm font-medium text-fg-subtle">Chats in this project</h2>
          <ul className="divide-y divide-line border-y border-line">
            {chats.map((c) => (
              <li key={c.id}>
                <Link
                  to={`/chat/${c.id}`}
                  className="-mx-3 flex items-baseline justify-between gap-4 rounded-lg px-3 py-3 hover:bg-hover"
                >
                  <span className="truncate text-md text-fg">{c.title}</span>
                  <span className="shrink-0 text-sm text-fg-subtle">
                    {formatRelative(c.updatedAt)}
                  </span>
                </Link>
              </li>
            ))}
            {chats.length === 0 && (
              <li className="py-6 text-center text-fg-muted">No chats yet. Start one above.</li>
            )}
          </ul>
        </div>

        <aside className="flex flex-col gap-3">
          <Card
            title="Instructions"
            action={
              <IconButton size="sm" label="Edit instructions" disabled>
                <Pencil className="size-4" />
              </IconButton>
            }
          >
            <p className="line-clamp-6 text-base text-fg-muted">{project.instructions}</p>
          </Card>
          <Card
            title="Knowledge"
            action={
              <IconButton size="sm" label="Add knowledge" disabled>
                <Plus className="size-4" />
              </IconButton>
            }
          >
            {project.knowledge.length === 0 ? (
              <p className="text-base text-fg-subtle">No files yet.</p>
            ) : (
              <ul className="flex flex-col gap-1.5">
                {project.knowledge.map((k) => (
                  <li key={k.name} className="flex items-center gap-2 text-base">
                    <FileText className="size-4 shrink-0 text-fg-muted" />
                    <span className="min-w-0 flex-1 truncate text-fg">{k.name}</span>
                    <span className="text-xs text-fg-subtle">{formatBytes(k.bytes)}</span>
                  </li>
                ))}
              </ul>
            )}
          </Card>
        </aside>
      </div>
    </div>
  );
}

function Card({
  title,
  action,
  children,
}: {
  title: string;
  action?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section className="rounded-xl border border-line bg-raised p-4 shadow-sm">
      <div className="mb-2 flex items-center justify-between">
        <h2 className="text-md font-semibold text-fg">{title}</h2>
        {action}
      </div>
      {children}
    </section>
  );
}
