import {
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  Info,
  Keyboard,
  PanelLeft,
  PanelRight,
  Plus,
  Search,
  Settings,
  Star,
} from "lucide-react";
import { useRef, useState } from "react";
import { Link, matchPath, useNavigate } from "react-router";
import { shortcutText } from "@/app/shortcuts";
import { InlineRename } from "@/components/InlineRename";
import { OxideMark } from "@/components/OxideMark";
import { IconButton } from "@/components/ui/IconButton";
import { MenuContent, MenuItem, MenuRoot, MenuSeparator, MenuTrigger } from "@/components/ui/Menu";
import { Tooltip } from "@/components/ui/Tooltip";
import { ChatMenuItems } from "@/features/chat/ChatMenuItems";
import { useExportChat } from "@/features/chat/useExportChat";
import {
  useConversation,
  useDeleteConversation,
  useRenameConversation,
  useStarConversation,
} from "@/features/chats/queries";
import { useApps } from "@/features/apps/queries";
import { useProject, useProjects } from "@/features/projects/queries";
import { useOpenSettings } from "@/features/settings/useOpenSettings";
import { StepPill } from "@/features/workspace/StepPill";
import { useSessionPanel } from "@/features/workspace/store";
import { cn } from "@/lib/cn";
import { platform } from "@/lib/platform";
import { useUi } from "@/stores/ui";
import { WindowControls } from "./WindowControls";

/**
 * Full-width top bar. The left segment lines up with the sidebar (same width and colour);
 * the rest shows the page breadcrumb, page actions and, on Windows, the caption buttons.
 * Empty areas carry `data-tauri-drag-region` so the window can be dragged and double-click
 * maximizes. macOS keeps an inset for the traffic lights; Linux uses native decorations.
 */
export function TitleBar({ pagePath }: { pagePath: string }) {
  const sidebarOpen = useUi((s) => s.sidebarOpen);
  const sidebarWidth = useUi((s) => s.sidebarWidth);
  const toggleSidebar = useUi((s) => s.toggleSidebar);
  const navigate = useNavigate();

  const chatMatch = matchPath("/chat/:id", pagePath);
  const historyIdx = (window.history.state as { idx?: number } | null)?.idx ?? 0;

  return (
    <header
      data-tauri-drag-region
      className="flex h-titlebar shrink-0 items-stretch border-b border-line bg-app"
    >
      <div
        data-tauri-drag-region
        className={cn(
          "flex shrink-0 items-center gap-0.5 px-2",
          sidebarOpen && "border-r border-line bg-sidebar",
        )}
        style={sidebarOpen ? { width: sidebarWidth } : undefined}
      >
        {platform === "macos" && (
          <div data-tauri-drag-region className="w-[var(--ox-mac-traffic-inset)] shrink-0" />
        )}
        <AppMenu />
        <IconButton
          label={sidebarOpen ? "Hide sidebar" : "Show sidebar"}
          shortcut={shortcutText("toggleSidebar")}
          onClick={toggleSidebar}
        >
          <PanelLeft className="size-[18px]" />
        </IconButton>
        <IconButton label="Back" disabled={historyIdx === 0} onClick={() => navigate(-1)}>
          <ChevronLeft className="size-[18px]" />
        </IconButton>
        <IconButton label="Forward" onClick={() => navigate(1)}>
          <ChevronRight className="size-[18px]" />
        </IconButton>
        <div data-tauri-drag-region className="h-full flex-1" />
      </div>

      <div data-tauri-drag-region className="flex min-w-0 flex-1 items-center gap-2 pl-4">
        <Breadcrumb pagePath={pagePath} />
        <div data-tauri-drag-region className="h-full min-w-4 flex-1" />
        {chatMatch?.params.id && <ChatPanelButtons conversationId={chatMatch.params.id} />}
        <div className={cn(platform === "windows" ? "w-1" : "w-2")} />
      </div>

      {platform === "windows" && <WindowControls />}
    </header>
  );
}

/** The "Step n of m" pill and the session panel button of a chat. */
function ChatPanelButtons({ conversationId }: { conversationId: string }) {
  const panel = useSessionPanel(conversationId);
  return (
    <>
      <StepPill conversationId={conversationId} />
      <IconButton
        label={panel.open ? "Hide chat details" : "Show chat details"}
        active={panel.open}
        onClick={panel.toggle}
      >
        <PanelRight className="size-[18px]" />
      </IconButton>
    </>
  );
}

function AppMenu() {
  const navigate = useNavigate();
  const openSettings = useOpenSettings();
  const setSearchOpen = useUi((s) => s.setSearchOpen);
  const setShortcutsOpen = useUi((s) => s.setShortcutsOpen);
  return (
    <MenuRoot>
      <Tooltip label="Oxide menu">
        <MenuTrigger asChild>
          <button
            type="button"
            aria-label="Oxide menu"
            className="inline-flex h-8 w-8 items-center justify-center rounded-md hover:bg-hover"
          >
            <OxideMark className="size-[20px]" />
          </button>
        </MenuTrigger>
      </Tooltip>
      <MenuContent align="start">
        <MenuItem shortcut={shortcutText("newChat")} onSelect={() => navigate("/new")}>
          <Plus />
          New chat
        </MenuItem>
        <MenuItem shortcut={shortcutText("search")} onSelect={() => setSearchOpen(true)}>
          <Search />
          Search chats
        </MenuItem>
        <MenuSeparator />
        <MenuItem shortcut={shortcutText("settings")} onSelect={() => openSettings("general")}>
          <Settings />
          Settings
        </MenuItem>
        <MenuItem shortcut={shortcutText("shortcuts")} onSelect={() => setShortcutsOpen(true)}>
          <Keyboard />
          Keyboard shortcuts
        </MenuItem>
        <MenuItem onSelect={() => openSettings("about")}>
          <Info />
          About Oxide
        </MenuItem>
      </MenuContent>
    </MenuRoot>
  );
}

const PAGE_TITLES: Array<[string, string]> = [
  ["/chats", "Chats"],
  ["/projects", "Projects"],
  ["/apps", "Utility Apps"],
  ["/artifacts", "Artifacts"],
];

function Breadcrumb({ pagePath }: { pagePath: string }) {
  const chat = matchPath("/chat/:id", pagePath);
  const project = matchPath("/project/:id", pagePath);
  const { project: p } = useProject(project?.params.id);
  const utilityApp = matchPath("/apps/:id", pagePath);
  const building = matchPath("/apps/:id/build", pagePath);
  const { data: apps } = useApps();

  if (chat?.params.id) return <ChatBreadcrumb id={chat.params.id} />;

  if (building?.params.id) {
    return (
      <nav aria-label="Breadcrumb" className="flex min-w-0 items-center gap-2 text-md">
        <Link to="/apps" className="shrink-0 text-fg-muted hover:text-fg">
          Utility Apps
        </Link>
        <span className="text-fg-subtle">/</span>
        <span className="truncate text-fg">Builder</span>
      </nav>
    );
  }

  if (utilityApp?.params.id) {
    const app = apps?.find((a) => a.manifest.id === utilityApp.params.id);
    return (
      <nav aria-label="Breadcrumb" className="flex min-w-0 items-center gap-2 text-md">
        <Link to="/apps" className="shrink-0 text-fg-muted hover:text-fg">
          Utility Apps
        </Link>
        <span className="text-fg-subtle">/</span>
        <span className="truncate text-fg">{app?.manifest.name ?? "App"}</span>
      </nav>
    );
  }

  if (project?.params.id) {
    return (
      <nav aria-label="Breadcrumb" className="flex min-w-0 items-center gap-2 text-md">
        <Link to="/projects" className="shrink-0 text-fg-muted hover:text-fg">
          Projects
        </Link>
        <span className="text-fg-subtle">/</span>
        <span className="truncate text-fg">{p?.name ?? "Unknown project"}</span>
      </nav>
    );
  }

  const title = PAGE_TITLES.find(([path]) => pagePath === path)?.[1];
  return title ? <span className="text-md text-fg">{title}</span> : null;
}

function ChatBreadcrumb({ id }: { id: string }) {
  const { conversation: convo } = useConversation(id);
  const { data: projects } = useProjects();
  const star = useStarConversation();
  const rename = useRenameConversation();
  const remove = useDeleteConversation();
  const exportChat = useExportChat();
  const navigate = useNavigate();
  const [renaming, setRenaming] = useState(false);
  const renameRequested = useRef(false);

  if (!convo) return null;
  const project = convo.projectId ? projects.find((p) => p.id === convo.projectId) : undefined;

  return (
    <nav aria-label="Breadcrumb" className="flex min-w-0 items-center gap-2 text-md">
      {project && (
        <>
          <Link
            to={`/project/${project.id}`}
            className="max-w-[40%] shrink truncate text-fg-muted hover:text-fg"
          >
            {project.name}
          </Link>
          <span className="text-fg-subtle">/</span>
        </>
      )}
      {renaming ? (
        <InlineRename
          value={convo.title}
          className="h-8 w-72"
          onCommit={(t) => {
            rename.mutate({ id, title: t });
            setRenaming(false);
          }}
          onCancel={() => setRenaming(false)}
        />
      ) : (
        <MenuRoot>
          <MenuTrigger asChild>
            <button
              type="button"
              className="flex min-w-0 items-center gap-1.5 rounded-md px-1.5 py-1 text-fg hover:bg-hover"
            >
              {convo.starred && <Star className="size-3.5 shrink-0 fill-current text-accent" />}
              <span className="truncate">{convo.title}</span>
              <ChevronDown className="size-4 shrink-0 text-fg-muted" />
            </button>
          </MenuTrigger>
          <MenuContent
            align="start"
            // Don't pull focus back to the trigger when the rename field is taking over.
            onCloseAutoFocus={(e) => {
              if (renameRequested.current) e.preventDefault();
              renameRequested.current = false;
            }}
          >
            <ChatMenuItems
              starred={convo.starred}
              onStar={() => star.mutate({ id, starred: !convo.starred })}
              onRename={() => {
                renameRequested.current = true;
                setRenaming(true);
              }}
              onExport={(format) => exportChat(id, format)}
              conversation={convo}
              onDelete={() => {
                remove.mutate(id);
                navigate("/new");
              }}
            />
          </MenuContent>
        </MenuRoot>
      )}
    </nav>
  );
}
