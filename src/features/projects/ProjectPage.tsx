import { Pencil, Plus } from "lucide-react";
import { useState, type ReactNode } from "react";
import { Link, useNavigate, useParams } from "react-router";
import { IconButton } from "@/components/ui/IconButton";
import { useConversations, useStartConversation } from "@/features/chats/queries";
import { Composer } from "@/features/composer/Composer";
import { useGenerateOptions } from "@/features/composer/options";
import { useNewChatModel } from "@/features/models/queries";
import { errorMessage } from "@/lib/api";
import { formatRelative } from "@/lib/time";
import { useProject } from "./queries";

export function ProjectPage() {
  const { id = "" } = useParams();
  const { project, isPending } = useProject(id);
  const { data: conversations } = useConversations();
  const start = useStartConversation();
  const [modelId, setModelId] = useNewChatModel();
  const options = useGenerateOptions();
  const navigate = useNavigate();
  const [draft, setDraft] = useState("");

  if (isPending) return <div className="flex-1" />;

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

  const chats = conversations.filter((c) => c.projectId === project.id);

  return (
    <div className="min-h-0 flex-1 overflow-y-auto">
      <div className="mx-auto grid w-full max-w-page grid-cols-1 gap-8 px-6 py-10 lg:grid-cols-[minmax(0,1fr)_18rem]">
        <div className="min-w-0">
          <h1 className="font-serif text-2xl font-normal tracking-tight text-fg">{project.name}</h1>
          {project.description && (
            <p className="mt-1 text-md text-fg-muted">{project.description}</p>
          )}
          <div className="mb-6" />
          <Composer
            variant="home"
            value={draft}
            onChange={setDraft}
            placeholder={`Start a chat in ${project.name}…`}
            modelId={modelId}
            onModelChange={setModelId}
            onSubmit={(text, attachments) => {
              start.mutate(
                {
                  input: {
                    text: text.trim(),
                    model: modelId,
                    projectId: project.id,
                    attachments,
                  },
                  options,
                },
                {
                  onSuccess: (c) => {
                    setDraft("");
                    navigate(`/chat/${c.id}`);
                  },
                },
              );
            }}
          />
          {start.error && (
            <p role="alert" className="mt-3 text-danger">
              Couldn’t start the chat: {errorMessage(start.error)}
            </p>
          )}
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
              <IconButton size="sm" label="Edit instructions (Phase 7)" disabled>
                <Pencil className="size-4" />
              </IconButton>
            }
          >
            <p className="line-clamp-6 text-base text-fg-muted">
              {project.instructions || "No instructions yet."}
            </p>
          </Card>
          <Card
            title="Knowledge"
            action={
              <IconButton size="sm" label="Add knowledge (Phase 7)" disabled>
                <Plus className="size-4" />
              </IconButton>
            }
          >
            <p className="text-base text-fg-subtle">No files yet.</p>
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
