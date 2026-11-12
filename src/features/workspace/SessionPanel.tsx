import {
  BookOpen,
  Box,
  Braces,
  Compass,
  CheckCircle2,
  Circle,
  CircleDot,
  ExternalLink,
  FileText,
  Folder,
  FolderOpen,
  FolderPlus,
  FolderSearch,
  Globe,
  Lightbulb,
  Plug,
  Shapes,
  ShieldCheck,
  Sparkles,
  SquareTerminal,
  X,
} from "lucide-react";
import type { ReactNode } from "react";
import { commands, type Message, type TaskItem } from "@/bindings";
import { IconButton } from "@/components/ui/IconButton";
import { Tooltip } from "@/components/ui/Tooltip";
import { attachmentsOf } from "@/features/chat/thread";
import { useToolSwitches } from "@/features/mcp/chatTools";
import { useMcpServers } from "@/features/mcp/queries";
import { EFFORT_LABELS, effectiveEffort, findModel } from "@/features/models/models";
import { useModelLabel, useModels } from "@/features/models/queries";
import { useChatGrants, useRevokeGrant, useToolUsage } from "@/features/tools/queries";
import { BrowserPanelSection } from "@/features/browser/BrowserPanel";
import { useBrowserEnabled } from "@/features/browser/queries";
import { SandboxSection } from "@/features/sandbox/SandboxPanel";
import { useShowSandbox } from "@/features/sandbox/queries";
import { usageRows, type UsageRow } from "@/features/tools/toolView";
import { errorMessage, unwrap } from "@/lib/api";
import { cn } from "@/lib/cn";
import { toast } from "@/lib/toast";
import { useUi } from "@/stores/ui";
import { useFolderControls } from "./folders";
import { grantLabel } from "./grants";
import { useChatWorkspace, useSetAllowDelete, useTaskList } from "./queries";
import { useSessionPanel } from "./store";
import { taskProgress } from "./tasks";

/**
 * The session panel beside a chat: the task list ("Progress"), shared folders, what the chat
 * used, tools allowed for the chat, and its model and files.
 */
export function SessionPanel({
  id,
  modelId,
  thread,
}: {
  id: string;
  modelId: string;
  thread: Message[];
}) {
  const panel = useSessionPanel(id);
  const showSandbox = useShowSandbox(id);
  const showBrowser = useBrowserEnabled();
  return (
    <aside aria-label="Chat details" className="flex w-80 shrink-0 flex-col p-3">
      <div className="flex min-h-0 flex-1 flex-col overflow-y-auto rounded-xl border border-line bg-raised p-4 shadow-sm">
        <div className="mb-2 flex items-center justify-between">
          <h2 className="text-md font-semibold text-fg">In this chat</h2>
          <IconButton size="sm" label="Close details" onClick={() => panel.setOpen(false)}>
            <X className="size-4" />
          </IconButton>
        </div>
        <Progress id={id} />
        <Folders id={id} />
        {showSandbox && (
          <SandboxSection
            id={id}
            wrap={(title, aside, body) => (
              <Section title={title} aside={aside}>
                {body}
              </Section>
            )}
          />
        )}
        {showBrowser && (
          <BrowserPanelSection
            id={id}
            wrap={(title, aside, body) => (
              <Section title={title} aside={aside}>
                {body}
              </Section>
            )}
          />
        )}
        <Section title="Used in this session">
          <UsedInSession id={id} />
        </Section>
        <Allowed id={id} />
        <Section title="Details">
          <Details id={id} modelId={modelId} thread={thread} />
        </Section>
      </div>
    </aside>
  );
}

function Section({
  title,
  aside,
  children,
}: {
  title: string;
  aside?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section aria-label={title} className="border-t border-line pt-3 first-of-type:border-t-0">
      <div className="mb-1.5 flex items-center justify-between gap-2">
        <h3 className="text-sm font-semibold text-fg">{title}</h3>
        {aside}
      </div>
      <div className="pb-3">{children}</div>
    </section>
  );
}

function Progress({ id }: { id: string }) {
  const { data } = useTaskList(id);
  const items = data?.items ?? [];
  const p = taskProgress(items);
  return (
    <Section
      title="Progress"
      aside={
        items.length > 0 ? (
          <span className="text-xs tabular-nums text-fg-subtle">
            {p.done} of {p.total} done
          </span>
        ) : null
      }
    >
      {items.length === 0 ? (
        <p className="text-sm text-fg-subtle">
          Longer tasks show their steps here as Oxide works through them.
        </p>
      ) : (
        <ol aria-label="Task list" className="flex flex-col gap-1">
          {items.map((t, i) => (
            <TaskRow key={`${i}-${t.title}`} task={t} current={i === p.current} />
          ))}
        </ol>
      )}
    </Section>
  );
}

function TaskRow({ task, current }: { task: TaskItem; current: boolean }) {
  const Icon =
    task.status === "done" ? CheckCircle2 : task.status === "inProgress" ? CircleDot : Circle;
  const label =
    task.status === "done" ? "Done" : task.status === "inProgress" ? "In progress" : "Not started";
  return (
    <li
      aria-current={current ? "step" : undefined}
      className="flex items-start gap-2 px-1 py-0.5 text-base"
    >
      <Icon
        aria-label={label}
        className={cn(
          "mt-0.5 size-4 shrink-0",
          task.status === "pending" ? "text-fg-subtle" : "text-accent",
        )}
      />
      <span
        className={cn(
          "min-w-0 flex-1",
          task.status === "done" ? "text-fg-subtle line-through" : "text-fg",
          current && "font-medium",
        )}
      >
        {task.title}
      </span>
    </li>
  );
}

function Folders({ id }: { id: string }) {
  const controls = useFolderControls(id);
  const { data: ws } = useChatWorkspace(id);
  const allowDelete = useSetAllowDelete(id);
  const open = (path: string) =>
    void unwrap(commands.openChatFolder(id, path)).catch((e: unknown) =>
      toast(errorMessage(e), "error"),
    );
  return (
    <Section
      title="Folders"
      aside={
        <button
          type="button"
          onClick={() => void controls.pick()}
          className="inline-flex h-7 items-center gap-1 rounded-md px-1.5 text-sm text-fg-muted hover:bg-hover hover:text-fg"
        >
          <FolderPlus className="size-3.5" />
          Add folder
        </button>
      }
    >
      {controls.folders.length === 0 ? (
        <p className="text-sm text-fg-subtle">
          Share a folder to let Oxide work with its files. It asks before changing anything.
        </p>
      ) : (
        <>
          <ul aria-label="Shared folders" className="flex flex-col gap-0.5">
            {controls.folders.map((f) => (
              <li key={f.path} className="group flex items-center gap-2 rounded-lg px-1 py-1">
                <Folder aria-hidden className="size-4 shrink-0 text-fg-subtle" />
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-base text-fg">{f.name}</span>
                  <span className="block truncate text-xs text-fg-subtle" title={f.path}>
                    {f.path}
                  </span>
                </span>
                <IconButton size="sm" label={`Open ${f.name}`} onClick={() => open(f.path)}>
                  <ExternalLink className="size-3.5" />
                </IconButton>
                <IconButton
                  size="sm"
                  label={`Stop sharing ${f.name}`}
                  onClick={() => controls.remove(f.path)}
                >
                  <X className="size-3.5" />
                </IconButton>
              </li>
            ))}
          </ul>
          <label className="mt-2 flex items-start gap-2 px-1 text-sm text-fg-muted">
            <input
              type="checkbox"
              className="mt-0.5 accent-accent"
              checked={ws?.allowDelete ?? false}
              onChange={(e) =>
                allowDelete.mutate(e.target.checked, {
                  onError: (err) => toast(errorMessage(err), "error"),
                })
              }
            />
            <span>
              Delete for good
              <span className="block text-xs text-fg-subtle">
                Off: files Oxide deletes go to a _to_delete folder inside the shared folder.
              </span>
            </span>
          </label>
        </>
      )}
    </Section>
  );
}

function UsageIcon({ kind }: { kind: UsageRow["kind"] }) {
  const icons: Partial<Record<UsageRow["kind"], typeof Plug>> = {
    webSearch: Globe,
    webFetch: FileText,
    providerTool: Sparkles,
    projectSearch: FolderSearch,
    artifact: Shapes,
    skill: BookOpen,
    file: FolderOpen,
    shell: SquareTerminal,
    python: Braces,
    sandbox: Box,
    browser: Compass,
  };
  const Icon = icons[kind] ?? Plug;
  return <Icon aria-hidden className="size-4" />;
}

/** Web searches, pages, connectors, files and commands. Click a row to see the call. */
function UsedInSession({ id }: { id: string }) {
  const { data } = useToolUsage(id);
  const rows = usageRows(data ?? []);
  if (rows.length === 0) {
    return <p className="text-sm text-fg-subtle">Nothing yet.</p>;
  }
  return (
    <ul aria-label="Used in this session" className="flex flex-col gap-1">
      {rows.map((r) => (
        <li key={r.key}>
          <button
            type="button"
            onClick={() =>
              document
                .querySelector(`[data-tool-call="${CSS.escape(r.toolUseId)}"]`)
                ?.scrollIntoView({ behavior: "smooth", block: "center" })
            }
            className="flex w-full items-start gap-2.5 rounded-lg px-2 py-1.5 text-left hover:bg-hover"
          >
            <span className="mt-0.5 text-fg-subtle">
              <UsageIcon kind={r.kind} />
            </span>
            <span className="min-w-0 flex-1">
              <span className="flex items-center gap-1.5 text-base text-fg">
                <span className="truncate">{r.title}</span>
                {r.count > 1 && (
                  <span className="shrink-0 rounded bg-sunken px-1.5 text-xs tabular-nums text-fg-subtle">
                    {r.count}
                  </span>
                )}
                {r.errors > 0 && (
                  <span className="shrink-0 text-xs text-danger">{r.errors} failed</span>
                )}
              </span>
              {r.details.length > 0 && (
                <span className="block truncate text-xs text-fg-subtle">
                  {r.details.slice(-3).join(" · ")}
                </span>
              )}
            </span>
          </button>
        </li>
      ))}
    </ul>
  );
}

/** "Allow always for this chat" choices, each with a way to take it back. */
function Allowed({ id }: { id: string }) {
  const { data } = useChatGrants(id);
  const revoke = useRevokeGrant(id);
  const grants = data ?? [];
  if (grants.length === 0) return null;
  return (
    <Section
      title="Allowed in this chat"
      aside={
        <button
          type="button"
          onClick={() => revoke.mutate(null)}
          className="h-7 rounded-md px-1.5 text-sm text-fg-muted hover:bg-hover hover:text-fg"
        >
          Revoke all
        </button>
      }
    >
      <ul aria-label="Allowed tools" className="flex flex-col gap-0.5">
        {grants.map((g) => (
          <li key={g} className="flex items-center gap-2 px-1 py-0.5 text-base">
            <ShieldCheck aria-hidden className="size-4 shrink-0 text-fg-subtle" />
            <Tooltip label={g}>
              <span className="min-w-0 flex-1 truncate text-fg">{grantLabel(g)}</span>
            </Tooltip>
            <IconButton
              size="sm"
              label={`Ask again for ${grantLabel(g)}`}
              onClick={() => revoke.mutate(g)}
            >
              <X className="size-3.5" />
            </IconButton>
          </li>
        ))}
      </ul>
    </Section>
  );
}

/** Which tools the chat may use: web and running connectors. */
function useToolsSummary(id: string): string {
  const switches = useToolSwitches(id);
  const { data: servers } = useMcpServers();
  const on = servers.filter((s) => s.status === "running" && switches.serverOn(s.name));
  const parts: string[] = [];
  if (switches.webOn) parts.push("Web");
  if (on.length > 0) parts.push(on.map((s) => s.name).join(", "));
  return parts.length > 0 ? parts.join(" · ") : "None";
}

function Details({ id, modelId, thread }: { id: string; modelId: string; thread: Message[] }) {
  const toolsSummary = useToolsSummary(id);
  const thinking = useUi((s) => s.thinking);
  const effort = useUi((s) => s.effort);
  const modelLabel = useModelLabel(modelId);
  const { data: lists } = useModels();
  const caps = findModel(lists, modelId)?.capabilities;
  const applied = caps ? effectiveEffort(caps, effort) : null;
  const files = thread.flatMap((m) =>
    attachmentsOf(m).map((b) => (b.type === "document" ? b.name : "Image")),
  );
  return (
    <dl className="flex flex-col gap-3 text-base">
      <Row icon={<Lightbulb className="size-4" />} label="Model">
        {modelLabel}
        {thinking ? " · thinking" : ""}
        {applied ? ` · ${EFFORT_LABELS[applied]} effort` : ""}
      </Row>
      <Row icon={<Plug className="size-4" />} label="Tools">
        {toolsSummary}
      </Row>
      <Row icon={<FileText className="size-4" />} label="Files">
        {files.length === 0 ? "None yet" : `${files.length}: ${files.join(", ")}`}
      </Row>
    </dl>
  );
}

function Row({ icon, label, children }: { icon: ReactNode; label: string; children: ReactNode }) {
  return (
    <div className="flex items-start gap-3">
      <span className="mt-0.5 text-fg-subtle">{icon}</span>
      <div className="min-w-0">
        <dt className="text-xs text-fg-subtle">{label}</dt>
        <dd className="truncate text-fg">{children}</dd>
      </div>
    </div>
  );
}
