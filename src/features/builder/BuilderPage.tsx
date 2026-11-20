import { useQueryClient } from "@tanstack/react-query";
import {
  ArrowLeft,
  Check,
  Code2,
  Download,
  Eye,
  History,
  Loader2,
  MoreHorizontal,
  Pencil,
  RotateCw,
  Save,
  Trash2,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { Link, useNavigate, useParams } from "react-router";
import { commands, events, type AppDraft } from "@/bindings";
import { IconButton } from "@/components/ui/IconButton";
import {
  MenuContent,
  MenuItem,
  MenuLabel,
  MenuRoot,
  MenuSeparator,
  MenuTrigger,
} from "@/components/ui/Menu";
import { AppIcon } from "@/features/apps/AppIcon";
import { useExportApp } from "@/features/apps/queries";
import { useBranches } from "@/features/chat/branches";
import { ChatView } from "@/features/chat/ChatPage";
import { useStreams } from "@/features/chat/streams";
import { activeThread } from "@/features/chat/thread";
import { useMessages, useSendMessage } from "@/features/chats/queries";
import { useGenerateOptions } from "@/features/composer/options";
import { useModels, useNewChatModel } from "@/features/models/queries";
import { errorMessage, unwrap } from "@/lib/api";
import { cn } from "@/lib/cn";
import { formatRelative } from "@/lib/time";
import { toast } from "@/lib/toast";
import { CodePane } from "./CodePane";
import { DetailsDialog } from "./DetailsDialog";
import { PreviewPane } from "./PreviewPane";
import {
  builderKeys,
  fixMessage,
  useBuilderChat,
  useCheckpoints,
  useDiscardDraft,
  useDraft,
  useOpenBuilder,
  usePublishDraft,
  useRestoreCheckpoint,
} from "./queries";

/** The primary action (its own colours: mixing them with `headerButton`'s clashed). */
const primaryButton =
  "inline-flex h-8 items-center gap-1.5 rounded-lg bg-fg px-3 text-sm font-medium text-app hover:opacity-90 disabled:opacity-40 [&_svg]:size-4";

const headerButton =
  "inline-flex h-8 items-center gap-1.5 rounded-lg border border-line px-3 text-sm text-fg hover:bg-hover disabled:opacity-40 [&_svg]:size-4";

/**
 * The Utility App builder (`/apps/:id/build`): a chat that builds the app on the left, and
 * the draft's Preview and Code on the right (Phase 21).
 */
export function BuilderPage() {
  const { id = "" } = useParams();
  const { data: models, isPending: modelsPending } = useModels();
  const [model] = useNewChatModel();
  const open = useOpenBuilder();
  const [opened, setOpened] = useState(false);
  const started = useRef("");

  // Opening makes the draft (from the saved app) and the chat the first time; after that it
  // only returns them.
  useEffect(() => {
    if (!id || !model || started.current === id) return;
    started.current = id;
    open.mutate(
      { id, model },
      { onSuccess: () => setOpened(true), onError: () => setOpened(false) },
    );
    // `open` is a stable mutation object.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [id, model]);

  const noModels = !modelsPending && models.every((l) => l.models.length === 0) && !model;
  if (noModels) {
    return (
      <Centered>
        <p className="text-md text-fg">The builder needs a chat model.</p>
        <Link to="/settings/providers" className="text-accent hover:underline">
          Add one in Settings → Providers
        </Link>
      </Centered>
    );
  }
  if (open.error) {
    return (
      <Centered>
        <p className="text-md text-fg">{errorMessage(open.error)}</p>
        <Link to="/apps" className="text-accent hover:underline">
          Back to Utility Apps
        </Link>
      </Centered>
    );
  }
  if (!opened) {
    return (
      <Centered>
        <Loader2 className="size-5 animate-spin text-fg-subtle" aria-label="Opening the builder" />
      </Centered>
    );
  }
  return <Builder key={id} id={id} />;
}

function Centered({ children }: { children: ReactNode }) {
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-3 p-6 text-center">
      {children}
    </div>
  );
}

function Builder({ id }: { id: string }) {
  const qc = useQueryClient();
  const navigate = useNavigate();
  const { data: draft, error } = useDraft(id);
  const cid = draft?.conversationId;
  const { data: chat } = useBuilderChat(cid);
  const [tab, setTab] = useState<"preview" | "code">("preview");
  const [details, setDetails] = useState(false);
  const filesKey = useFilesKey(draft);

  // The model changed files or a reply ended (with a checkpoint).
  useEffect(() => {
    let closed = false;
    const stops: Array<() => void> = [];
    const keep = (p: Promise<() => void>) =>
      p.then((u) => (closed ? u() : stops.push(u))).catch(() => {});
    keep(
      events.appDraftChanged.listen(({ payload }) => {
        if (payload.appId === id) void qc.invalidateQueries({ queryKey: builderKeys.draft(id) });
      }),
    );
    keep(
      events.chatStream.listen(({ payload }) => {
        if (payload.conversationId !== cid) return;
        const t = payload.event.type;
        if (t === "done" || t === "failed") {
          void qc.invalidateQueries({ queryKey: builderKeys.draft(id) });
          void qc.invalidateQueries({ queryKey: builderKeys.checkpoints(id) });
        }
      }),
    );
    return () => {
      closed = true;
      stops.forEach((s) => s());
    };
  }, [id, cid, qc]);

  const fix = useFixWithAi(cid);

  if (error) {
    return (
      <Centered>
        <p className="text-md text-fg">{errorMessage(error)}</p>
        <Link to="/apps" className="text-accent hover:underline">
          Back to Utility Apps
        </Link>
      </Centered>
    );
  }
  if (!draft || !chat) return <div className="flex-1" />;
  const m = draft.app.manifest;

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <BuilderHeader
        draft={draft}
        onDetails={() => setDetails(true)}
        onLeave={() => navigate(draft.published ? `/apps/${id}` : "/apps")}
      />
      <div className="flex min-h-0 flex-1">
        <section
          aria-label="Builder chat"
          className="flex w-[min(440px,40%)] min-w-[320px] shrink-0 flex-col border-r border-line"
        >
          <ChatView
            id={chat.id}
            conversation={chat}
            embedded
            onModelChange={(model) =>
              void unwrap(commands.setConversationModel(chat.id, model))
                .then((c) => qc.setQueryData(builderKeys.chat(chat.id), c))
                .catch((e: unknown) => toast(errorMessage(e), "error"))
            }
            placeholder={<BuilderWelcome name={m.name} />}
          />
        </section>
        <section aria-label="App" className="flex min-w-0 flex-1 flex-col">
          <div className="flex h-10 shrink-0 items-center gap-2 border-b border-line px-3">
            <div role="tablist" aria-label="View" className="flex rounded-lg bg-sunken p-0.5">
              {(
                [
                  ["preview", "Preview", Eye],
                  ["code", "Code", Code2],
                ] as const
              ).map(([key, label, Icon]) => (
                <button
                  key={key}
                  type="button"
                  role="tab"
                  aria-selected={tab === key}
                  onClick={() => setTab(key)}
                  className={cn(
                    "inline-flex h-7 items-center gap-1.5 rounded-md px-3 text-sm",
                    tab === key ? "bg-raised text-fg shadow-sm" : "text-fg-muted hover:text-fg",
                  )}
                >
                  <Icon className="size-3.5" />
                  {label}
                </button>
              ))}
            </div>
            <span className="flex-1" />
            {tab === "preview" && (
              <IconButton
                size="sm"
                label="Reload the preview"
                onClick={() =>
                  void qc.invalidateQueries({ queryKey: builderKeys.draft(id) }).then(filesKey.bump)
                }
              >
                <RotateCw className="size-4" />
              </IconButton>
            )}
          </div>
          <div className="relative flex min-h-0 flex-1">
            {/* The preview stays mounted (and laid out) behind Code, so the builder's model can
                check it and take screenshots while the user reads code. */}
            <div
              role="tabpanel"
              aria-label="Preview"
              className={cn("absolute inset-0 flex flex-col", tab !== "preview" && "invisible")}
            >
              <PreviewPane draft={draft} filesKey={filesKey.value} onFixError={fix} />
            </div>
            {tab === "code" && (
              <div role="tabpanel" aria-label="Code" className="absolute inset-0 flex bg-app">
                <CodePane draft={draft} />
              </div>
            )}
          </div>
        </section>
      </div>
      <DetailsDialog draft={draft} open={details} onOpenChange={setDetails} />
    </div>
  );
}

/** A counter that goes up (after a short pause) whenever the draft's files change. */
function useFilesKey(draft: AppDraft | undefined) {
  const [value, setValue] = useState(0);
  const files = draft?.files;
  const first = useRef(true);
  useEffect(() => {
    if (!files) return;
    if (first.current) {
      first.current = false;
      return;
    }
    const t = setTimeout(() => setValue((v) => v + 1), 300);
    return () => clearTimeout(t);
  }, [files]);
  return useMemo(() => ({ value, bump: () => setValue((v) => v + 1) }), [value]);
}

/** Sends **Fix with AI** to the builder chat. */
function useFixWithAi(cid: string | undefined) {
  const { data: messages } = useMessages(cid ?? "");
  const pickedLeaf = useBranches((s) => (cid ? s.leaf[cid] : undefined));
  const live = useStreams((s) => (cid ? s.live[cid] !== undefined : false));
  const send = useSendMessage();
  const options = useGenerateOptions();
  return (error: string) => {
    if (!cid) return;
    if (live) {
      toast("The builder is still working. Try again when it's done.");
      return;
    }
    const thread = activeThread(messages ?? [], pickedLeaf);
    send.mutate(
      {
        conversationId: cid,
        parentId: thread[thread.length - 1]?.id ?? null,
        text: fixMessage(error),
        attachments: [],
        options,
      },
      { onError: (e) => toast(errorMessage(e), "error") },
    );
  };
}

function BuilderWelcome({ name }: { name: string }) {
  return (
    <div className="flex flex-col gap-2 rounded-xl border border-dashed border-line-strong p-4 text-base text-fg-muted">
      <p className="font-medium text-fg">Build {name} with AI</p>
      <p>
        Describe the app you want, or what to change. The builder writes the code; the Preview
        updates as it works, and every step is saved as a checkpoint you can go back to.
      </p>
      <p className="text-sm text-fg-subtle">
        Try: “An app that turns a pasted CSV into a chart and lets me download it as PNG.”
      </p>
    </div>
  );
}

function BuilderHeader({
  draft,
  onDetails,
  onLeave,
}: {
  draft: AppDraft;
  onDetails: () => void;
  onLeave: () => void;
}) {
  const navigate = useNavigate();
  const m = draft.app.manifest;
  const publish = usePublishDraft(m.id);
  const discard = useDiscardDraft();
  const exportApp = useExportApp();
  const status = !draft.published ? "Draft" : draft.unsaved ? "Unsaved changes" : "Saved";

  return (
    <header className="flex items-center gap-3 border-b border-line px-3 py-2">
      <IconButton label="Close the builder" onClick={onLeave}>
        <ArrowLeft className="size-4" />
      </IconButton>
      <AppIcon icon={m.icon ?? "sparkles"} className="size-8" />
      <button
        type="button"
        onClick={onDetails}
        aria-label="App details"
        className="group flex min-w-0 flex-1 flex-col items-start text-left"
      >
        <span className="flex max-w-full items-center gap-1.5 text-md font-medium text-fg">
          <span className="truncate">{m.name}</span>
          <Pencil className="size-3.5 shrink-0 text-fg-subtle opacity-0 group-hover:opacity-100" />
        </span>
        <span className="truncate text-sm text-fg-subtle">
          Builder · {m.category} ·{" "}
          <span className={cn(status !== "Saved" && "text-accent")}>{status}</span>
        </span>
      </button>
      <CheckpointsMenu id={m.id} />
      {draft.published && !draft.unsaved ? (
        <span
          role="status"
          className="inline-flex h-8 items-center gap-1.5 rounded-lg px-3 text-sm text-fg-muted [&_svg]:size-4"
        >
          <Check /> Saved
        </span>
      ) : (
        <button
          type="button"
          className={primaryButton}
          disabled={publish.isPending || Boolean(draft.app.problem)}
          title={draft.app.problem ? `The app can't run: ${draft.app.problem}` : undefined}
          onClick={() =>
            publish.mutate(undefined, {
              onSuccess: (app) =>
                toast(
                  draft.published
                    ? `Saved ${app.manifest.name}.`
                    : `Saved ${app.manifest.name} to Utility Apps.`,
                ),
              onError: (e) => toast(errorMessage(e), "error"),
            })
          }
        >
          <Save />
          Save to Utility Apps
        </button>
      )}
      <MenuRoot>
        <MenuTrigger asChild>
          <IconButton label="More" tooltip={false}>
            <MoreHorizontal className="size-4" />
          </IconButton>
        </MenuTrigger>
        <MenuContent align="end">
          <MenuItem onSelect={onDetails}>
            <Pencil /> Name, icon and category…
          </MenuItem>
          <MenuItem
            disabled={!draft.published}
            onSelect={() =>
              exportApp.mutate(m.id, {
                onSuccess: (path) => path && toast(`Exported the saved app to ${path}.`),
                onError: (e) => toast(errorMessage(e), "error"),
              })
            }
          >
            <Download /> Export saved app
          </MenuItem>
          <MenuSeparator />
          <MenuItem
            danger
            disabled={draft.published && !draft.unsaved}
            onSelect={() =>
              discard.mutate(m.id, {
                onSuccess: () => {
                  toast(draft.published ? "Changes discarded." : "Draft deleted.");
                  navigate(draft.published ? `/apps/${m.id}` : "/apps");
                },
                onError: (e) => toast(errorMessage(e), "error"),
              })
            }
          >
            <Trash2 /> {draft.published ? "Discard unsaved changes" : "Delete draft"}
          </MenuItem>
        </MenuContent>
      </MenuRoot>
    </header>
  );
}

function CheckpointsMenu({ id }: { id: string }) {
  const { data: checkpoints } = useCheckpoints(id);
  const restore = useRestoreCheckpoint(id);
  const list = checkpoints ?? [];
  return (
    <MenuRoot>
      <MenuTrigger asChild>
        <button type="button" className={headerButton} aria-label="Checkpoints">
          <History />
          Checkpoints
          {list.length > 0 && <span className="text-fg-subtle">{list.length}</span>}
        </button>
      </MenuTrigger>
      <MenuContent align="end" className="max-h-[60vh] w-80 overflow-y-auto">
        <MenuLabel>Restore a version (the current one is kept)</MenuLabel>
        {list.length === 0 && (
          <p className="px-2.5 py-2 text-sm text-fg-muted">No checkpoints yet.</p>
        )}
        {list.map((c, i) => (
          <MenuItem
            key={c.id}
            disabled={restore.isPending}
            onSelect={() =>
              restore.mutate(c.id, {
                onSuccess: () => toast(`Restored “${c.label}”.`),
                onError: (e) => toast(errorMessage(e), "error"),
              })
            }
          >
            <span className="flex min-w-0 flex-1 flex-col">
              <span className="truncate">{c.label}</span>
              <span className="text-xs text-fg-subtle">
                {i === 0 ? "Latest · " : ""}
                {formatRelative(c.createdAt)} · {c.files} files
              </span>
            </span>
          </MenuItem>
        ))}
      </MenuContent>
    </MenuRoot>
  );
}
