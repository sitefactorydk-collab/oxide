import {
  Archive,
  ArchiveRestore,
  Ellipsis,
  FileCode2,
  FileText,
  FolderMinus,
  NotebookPen,
  Pencil,
  Plus,
  Search,
  Trash2,
  Upload,
} from "lucide-react";
import { useRef, useState, type ChangeEvent, type ReactNode } from "react";
import { Link, useNavigate, useParams } from "react-router";
import { commands, type KnowledgeFile, type Project } from "@/bindings";
import { IconButton } from "@/components/ui/IconButton";
import { MenuContent, MenuItem, MenuRoot, MenuSeparator, MenuTrigger } from "@/components/ui/Menu";
import { Tooltip } from "@/components/ui/Tooltip";
import { baseName, readBase64 } from "@/features/attachments/useAttachments";
import { useConversations, useStartConversation } from "@/features/chats/queries";
import { Composer } from "@/features/composer/Composer";
import { useGenerateOptions } from "@/features/composer/options";
import { useNewChatModel } from "@/features/models/queries";
import { errorMessage } from "@/lib/api";
import { cn } from "@/lib/cn";
import { inTauri } from "@/lib/platform";
import { formatRelative } from "@/lib/time";
import { toast } from "@/lib/toast";
import { budgetPercent, formatTokens, knowledgeSummary } from "./knowledge";
import {
  AddTextDialog,
  ConfirmDialog,
  EditProjectDialog,
  InstructionsDialog,
  KnowledgeFileDialog,
} from "./ProjectDialogs";
import {
  patchOf,
  useAddProjectFiles,
  useDeleteKnowledgeFile,
  useDeleteProject,
  useProject,
  useProjectKnowledge,
  useSetConversationProject,
  useUpdateProject,
  type AddFilesResult,
} from "./queries";

/** Types the knowledge picker offers in a browser (the native picker has its own filter). */
const KNOWLEDGE_ACCEPT =
  ".pdf,.txt,.md,.markdown,.csv,.tsv,.json,.yaml,.yml,.toml,.xml,.html,.css,.js,.ts,.tsx,.jsx,.py,.rs,.go,.java,.c,.cpp,.h,.cs,.rb,.php,.sh,.ps1,.sql,.log";

export function ProjectPage() {
  const { id = "" } = useParams();
  const { project, isPending } = useProject(id);
  const { data: conversations } = useConversations();
  const start = useStartConversation();
  const [modelId, setModelId] = useNewChatModel();
  const options = useGenerateOptions();
  const navigate = useNavigate();
  const [draft, setDraft] = useState("");
  const move = useSetConversationProject();

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
      <div className="mx-auto grid w-full max-w-page grid-cols-1 gap-8 px-6 py-10 lg:grid-cols-[minmax(0,1fr)_20rem]">
        <div className="min-w-0">
          <Link to="/projects" className="text-sm text-fg-subtle hover:text-fg">
            ← All projects
          </Link>
          <div className="mt-2 flex items-start justify-between gap-3">
            <div className="min-w-0">
              <h1 className="font-serif text-2xl font-normal tracking-tight text-fg">
                {project.name}
              </h1>
              {project.description && (
                <p className="mt-1 text-md text-fg-muted">{project.description}</p>
              )}
            </div>
            <ProjectMenu project={project} />
          </div>
          {project.archived && <ArchivedBanner project={project} />}
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
              <li key={c.id} className="group relative">
                <Link
                  to={`/chat/${c.id}`}
                  className="-mx-3 flex items-baseline justify-between gap-4 rounded-lg px-3 py-3 pr-12 hover:bg-hover"
                >
                  <span className="truncate text-md text-fg">{c.title}</span>
                  <span className="shrink-0 text-sm text-fg-subtle">
                    {formatRelative(c.updatedAt)}
                  </span>
                </Link>
                <span className="absolute right-0 top-1/2 -translate-y-1/2 opacity-0 transition-opacity focus-within:opacity-100 group-hover:opacity-100">
                  <IconButton
                    size="sm"
                    label={`Remove “${c.title}” from the project`}
                    onClick={() => move.mutate({ id: c.id, projectId: null })}
                  >
                    <FolderMinus className="size-4" />
                  </IconButton>
                </span>
              </li>
            ))}
            {chats.length === 0 && (
              <li className="py-6 text-center text-fg-muted">No chats yet. Start one above.</li>
            )}
          </ul>
        </div>

        <aside className="flex flex-col gap-3">
          <InstructionsCard project={project} />
          <KnowledgeCard projectId={project.id} />
        </aside>
      </div>
    </div>
  );
}

function ArchivedBanner({ project }: { project: Project }) {
  const update = useUpdateProject();
  return (
    <div
      role="status"
      className="mt-4 flex items-center justify-between gap-3 rounded-xl border border-line bg-sunken px-4 py-3 text-base text-fg-muted"
    >
      <span>
        This project is archived. Its chats still work; it’s hidden from the project list.
      </span>
      <button
        type="button"
        onClick={() => update.mutate({ id: project.id, patch: patchOf({ archived: false }) })}
        className="h-8 shrink-0 rounded-lg border border-line px-3 text-sm text-fg hover:bg-hover"
      >
        Unarchive
      </button>
    </div>
  );
}

function ProjectMenu({ project }: { project: Project }) {
  const update = useUpdateProject();
  const remove = useDeleteProject();
  const navigate = useNavigate();
  const [editing, setEditing] = useState(false);
  const [confirming, setConfirming] = useState(false);
  return (
    <>
      <MenuRoot>
        <Tooltip label="Project options">
          <MenuTrigger asChild>
            <IconButton label="Project options" tooltip={false}>
              <Ellipsis className="size-4" />
            </IconButton>
          </MenuTrigger>
        </Tooltip>
        <MenuContent align="end">
          <MenuItem onSelect={() => setEditing(true)}>
            <Pencil />
            Edit details
          </MenuItem>
          <MenuItem
            onSelect={() =>
              update.mutate(
                { id: project.id, patch: patchOf({ archived: !project.archived }) },
                {
                  onSuccess: (p) => toast(p.archived ? "Project archived" : "Project unarchived"),
                },
              )
            }
          >
            {project.archived ? <ArchiveRestore /> : <Archive />}
            {project.archived ? "Unarchive" : "Archive"}
          </MenuItem>
          <MenuSeparator />
          <MenuItem danger onSelect={() => setConfirming(true)}>
            <Trash2 />
            Delete project
          </MenuItem>
        </MenuContent>
      </MenuRoot>
      <EditProjectDialog project={project} open={editing} onOpenChange={setEditing} />
      <ConfirmDialog
        open={confirming}
        onOpenChange={setConfirming}
        title={`Delete “${project.name}”?`}
        body="Its instructions, knowledge files and project memory are deleted. Its chats stay, outside any project."
        confirmLabel="Delete project"
        onConfirm={() =>
          remove.mutate(project.id, {
            onSuccess: () => navigate("/projects"),
          })
        }
      />
    </>
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
    <section aria-label={title} className="rounded-xl border border-line bg-raised p-4 shadow-sm">
      <div className="mb-2 flex items-center justify-between">
        <h2 className="text-md font-semibold text-fg">{title}</h2>
        {action}
      </div>
      {children}
    </section>
  );
}

function InstructionsCard({ project }: { project: Project }) {
  const [open, setOpen] = useState(false);
  return (
    <Card
      title="Instructions"
      action={
        <IconButton size="sm" label="Edit instructions" onClick={() => setOpen(true)}>
          <Pencil className="size-4" />
        </IconButton>
      }
    >
      {project.instructions ? (
        <p className="line-clamp-6 whitespace-pre-line text-base text-fg-muted">
          {project.instructions}
        </p>
      ) : (
        <button
          type="button"
          onClick={() => setOpen(true)}
          className="text-left text-base text-fg-subtle hover:text-fg"
        >
          Add instructions to tailor every chat in this project.
        </button>
      )}
      <InstructionsDialog project={project} open={open} onOpenChange={setOpen} />
    </Card>
  );
}

function fileIcon(f: KnowledgeFile) {
  if (f.kind === "note") return NotebookPen;
  if (f.kind === "pdf" || /\.(md|markdown|txt)$/i.test(f.name)) return FileText;
  return FileCode2;
}

function reportAdded(r: AddFilesResult) {
  if (r.added.length) {
    toast(
      r.added.length === 1
        ? `Added ${r.added[0]?.name ?? "the file"}`
        : `Added ${r.added.length} files`,
    );
  }
  for (const f of r.failed) toast(f.error, "error");
}

function KnowledgeCard({ projectId }: { projectId: string }) {
  const { data, error } = useProjectKnowledge(projectId);
  const add = useAddProjectFiles(projectId);
  const remove = useDeleteKnowledgeFile(projectId);
  const [viewing, setViewing] = useState<string | null>(null);
  const [addingText, setAddingText] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  const upload = () => {
    if (!inTauri) {
      inputRef.current?.click();
      return;
    }
    void commands.pickFiles().then((res) => {
      if (res.status !== "ok" || res.data.length === 0) return;
      add.mutate(
        res.data.map((path) => ({
          name: baseName(path),
          source: () => Promise.resolve({ kind: "path" as const, path }),
        })),
        { onSuccess: reportAdded },
      );
    });
  };

  const onInput = (e: ChangeEvent<HTMLInputElement>) => {
    const files = Array.from(e.target.files ?? []);
    e.target.value = "";
    if (files.length === 0) return;
    add.mutate(
      files.map((file) => ({
        name: file.name,
        source: async () => ({
          kind: "bytes" as const,
          name: file.name,
          data: await readBase64(file),
        }),
      })),
      { onSuccess: reportAdded },
    );
  };

  return (
    <Card
      title="Knowledge"
      action={
        <MenuRoot>
          <Tooltip label="Add knowledge">
            <MenuTrigger asChild>
              <IconButton size="sm" label="Add knowledge" tooltip={false}>
                <Plus className="size-4" />
              </IconButton>
            </MenuTrigger>
          </Tooltip>
          <MenuContent align="end">
            <MenuItem onSelect={upload}>
              <Upload />
              Upload files
            </MenuItem>
            <MenuItem onSelect={() => setAddingText(true)}>
              <NotebookPen />
              Add text content
            </MenuItem>
          </MenuContent>
        </MenuRoot>
      }
    >
      <input
        ref={inputRef}
        type="file"
        multiple
        accept={KNOWLEDGE_ACCEPT}
        aria-label="Upload knowledge files"
        className="hidden"
        onChange={onInput}
      />
      {error && <p className="text-base text-danger">{errorMessage(error)}</p>}
      {data && (
        <>
          {data.mode !== "empty" && (
            <div
              role="meter"
              aria-label="Knowledge budget used"
              aria-valuemin={0}
              aria-valuemax={100}
              aria-valuenow={budgetPercent(data)}
              className="mb-2 h-1.5 overflow-hidden rounded-full bg-sunken"
            >
              <div
                className={cn(
                  "h-full rounded-full",
                  data.mode === "search" ? "bg-fg-subtle" : "bg-accent",
                )}
                style={{ width: `${budgetPercent(data)}%` }}
              />
            </div>
          )}
          <p className="mb-2 flex items-start gap-1.5 text-sm text-fg-subtle">
            {data.mode === "search" && <Search aria-hidden className="mt-0.5 size-3.5 shrink-0" />}
            {knowledgeSummary(data)}
          </p>
          <ul className="-mx-2 flex flex-col">
            {data.files.map((f) => {
              const Icon = fileIcon(f);
              return (
                <li key={f.id} className="group flex items-center gap-1 rounded-lg hover:bg-hover">
                  <button
                    type="button"
                    onClick={() => setViewing(f.id)}
                    className="flex min-w-0 flex-1 items-center gap-2 px-2 py-1.5 text-left"
                  >
                    <Icon aria-hidden className="size-4 shrink-0 text-fg-muted" />
                    <span className="min-w-0 flex-1 truncate text-base text-fg">{f.name}</span>
                    <span className="shrink-0 text-xs text-fg-subtle">
                      {formatTokens(f.tokens)}
                    </span>
                  </button>
                  <span className="pr-1 opacity-0 transition-opacity focus-within:opacity-100 group-hover:opacity-100">
                    <IconButton
                      size="sm"
                      label={`Remove ${f.name}`}
                      onClick={() => remove.mutate(f.id)}
                    >
                      <Trash2 className="size-3.5" />
                    </IconButton>
                  </span>
                </li>
              );
            })}
          </ul>
          {add.isPending && <p className="mt-2 text-sm text-fg-muted">Adding files…</p>}
        </>
      )}
      <KnowledgeFileDialog fileId={viewing} onClose={() => setViewing(null)} />
      <AddTextDialog projectId={projectId} open={addingText} onOpenChange={setAddingText} />
    </Card>
  );
}
