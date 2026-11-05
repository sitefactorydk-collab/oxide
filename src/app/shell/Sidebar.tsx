import {
  ChevronDown,
  Ellipsis,
  FolderClosed,
  MessagesSquare,
  Plus,
  Search,
  Shapes,
  type LucideIcon,
} from "lucide-react";
import { useMemo, useRef, useState, type PointerEvent as ReactPointerEvent } from "react";
import { NavLink, useMatch, useNavigate } from "react-router";
import { shortcutText } from "@/app/shortcuts";
import { InlineRename } from "@/components/InlineRename";
import { MenuContent, MenuRoot, MenuTrigger } from "@/components/ui/Menu";
import { Tooltip } from "@/components/ui/Tooltip";
import { ChatMenuItems } from "@/features/chat/ChatMenuItems";
import { useExportChat } from "@/features/chat/useExportChat";
import type { Conversation } from "@/bindings";
import {
  useConversations,
  useDeleteConversation,
  useRenameConversation,
  useStarConversation,
} from "@/features/chats/queries";
import { cn } from "@/lib/cn";
import { SIDEBAR_DEFAULT, useUi } from "@/stores/ui";
import { ProfileMenu } from "./ProfileMenu";

export function Sidebar({ activeChatId }: { activeChatId?: string }) {
  const width = useUi((s) => s.sidebarWidth);
  const setSearchOpen = useUi((s) => s.setSearchOpen);
  const starredOpen = useUi((s) => s.starredOpen);
  const recentsOpen = useUi((s) => s.recentsOpen);
  const toggleSection = useUi((s) => s.toggleSection);
  const { data: conversations, isPending } = useConversations();

  const [starred, recents] = useMemo(() => {
    // The list arrives newest first.
    return [conversations.filter((c) => c.starred), conversations.filter((c) => !c.starred)];
  }, [conversations]);

  return (
    <aside
      aria-label="Sidebar"
      className="relative flex shrink-0 flex-col border-r border-line bg-sidebar"
      style={{ width }}
    >
      <nav aria-label="Primary" className="flex flex-col gap-0.5 px-2 pt-2">
        <NavItem to="/new" icon={Plus} label="New chat" shortcut={shortcutText("newChat")} />
        <Tooltip label="Search chats" shortcut={shortcutText("search")} side="right">
          <button type="button" onClick={() => setSearchOpen(true)} className={navItemClass(false)}>
            <Search className="size-[18px] shrink-0 text-fg-muted" />
            <span className="flex-1 text-left">Search</span>
          </button>
        </Tooltip>
        <NavItem to="/chats" icon={MessagesSquare} label="Chats" />
        <NavItem to="/projects" icon={FolderClosed} label="Projects" />
        <NavItem to="/artifacts" icon={Shapes} label="Artifacts" />
      </nav>

      <div className="mt-4 min-h-0 flex-1 overflow-y-auto px-2 pb-3">
        {starred.length > 0 && (
          <Section
            title="Starred"
            open={starredOpen}
            onToggle={() => toggleSection("starred")}
            items={starred}
            activeChatId={activeChatId}
          />
        )}
        <Section
          title="Recents"
          open={recentsOpen}
          onToggle={() => toggleSection("recents")}
          items={recents}
          activeChatId={activeChatId}
          empty={isPending ? undefined : "No chats yet"}
        />
      </div>

      <div className="border-t border-line p-2">
        <ProfileMenu />
      </div>

      <ResizeHandle />
    </aside>
  );
}

function navItemClass(active: boolean): string {
  return cn(
    "flex h-9 w-full items-center gap-3 rounded-lg px-2.5 text-md text-fg transition-colors",
    active ? "bg-active" : "hover:bg-hover",
  );
}

function NavItem({
  to,
  icon: Icon,
  label,
  shortcut,
}: {
  to: string;
  icon: LucideIcon;
  label: string;
  shortcut?: string;
}) {
  // Resolve the active state here: a Tooltip (Radix Slot) would stringify NavLink's
  // function-style className.
  const active = useMatch({ path: to, end: true }) !== null;
  const link = (
    <NavLink to={to} end className={navItemClass(active)}>
      <Icon className="size-[18px] shrink-0 text-fg-muted" />
      <span className="flex-1">{label}</span>
    </NavLink>
  );
  // Shortcuts live in tooltips, keeping the sidebar as quiet as the reference design.
  return shortcut ? (
    <Tooltip label={label} shortcut={shortcut} side="right">
      {link}
    </Tooltip>
  ) : (
    link
  );
}

function Section({
  title,
  open,
  onToggle,
  items,
  activeChatId,
  empty,
}: {
  title: string;
  open: boolean;
  onToggle: () => void;
  items: Conversation[];
  activeChatId?: string;
  empty?: string;
}) {
  return (
    <section className="mb-3">
      <button
        type="button"
        onClick={onToggle}
        aria-expanded={open}
        className="flex h-7 items-center gap-1 rounded-md px-2.5 text-sm font-medium text-fg-subtle hover:text-fg-muted"
      >
        {title}
        <ChevronDown className={cn("size-3.5 transition-transform", !open && "-rotate-90")} />
      </button>
      {open && (
        <ul className="mt-0.5 flex flex-col gap-px">
          {items.map((c) => (
            <ChatRow key={c.id} convo={c} active={c.id === activeChatId} />
          ))}
          {items.length === 0 && empty && (
            <li className="px-2.5 py-1.5 text-sm text-fg-subtle">{empty}</li>
          )}
        </ul>
      )}
    </section>
  );
}

function ChatRow({ convo, active }: { convo: Conversation; active: boolean }) {
  const star = useStarConversation();
  const rename = useRenameConversation();
  const remove = useDeleteConversation();
  const exportChat = useExportChat();
  const navigate = useNavigate();
  const [renaming, setRenaming] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);
  const renameRequested = useRef(false);

  if (renaming) {
    return (
      <li className="px-0.5 py-0.5">
        <InlineRename
          value={convo.title}
          className="h-8 w-full"
          onCommit={(t) => {
            rename.mutate({ id: convo.id, title: t });
            setRenaming(false);
          }}
          onCancel={() => setRenaming(false)}
        />
      </li>
    );
  }

  return (
    <li
      className={cn(
        "group relative flex h-9 items-center rounded-lg transition-colors",
        active ? "bg-active" : "hover:bg-hover",
        menuOpen && !active && "bg-hover",
      )}
    >
      <NavLink
        to={`/chat/${convo.id}`}
        title={convo.title}
        className="flex h-full min-w-0 flex-1 items-center pl-2.5 pr-8 text-md text-fg"
      >
        <span className="truncate">{convo.title}</span>
      </NavLink>
      <MenuRoot open={menuOpen} onOpenChange={setMenuOpen}>
        <Tooltip label="More options">
          <MenuTrigger asChild>
            <button
              type="button"
              aria-label={`More options for ${convo.title}`}
              className={cn(
                "absolute right-1 inline-flex h-7 w-7 items-center justify-center rounded-md text-fg-muted hover:bg-active hover:text-fg",
                menuOpen
                  ? "opacity-100"
                  : "opacity-0 focus-visible:opacity-100 group-hover:opacity-100",
              )}
            >
              <Ellipsis className="size-4" />
            </button>
          </MenuTrigger>
        </Tooltip>
        <MenuContent
          align="start"
          onCloseAutoFocus={(e) => {
            if (renameRequested.current) e.preventDefault();
            renameRequested.current = false;
          }}
        >
          <ChatMenuItems
            starred={convo.starred}
            onStar={() => star.mutate({ id: convo.id, starred: !convo.starred })}
            onRename={() => {
              renameRequested.current = true;
              setRenaming(true);
            }}
            onExport={(format) => exportChat(convo.id, format)}
            onDelete={() => {
              remove.mutate(convo.id);
              if (active) navigate("/new");
            }}
          />
        </MenuContent>
      </MenuRoot>
    </li>
  );
}

/** Drag the sidebar's right edge to resize; double-click resets. */
function ResizeHandle() {
  const setWidth = useUi((s) => s.setSidebarWidth);
  const [dragging, setDragging] = useState(false);

  const onPointerDown = (e: ReactPointerEvent<HTMLDivElement>) => {
    e.preventDefault();
    const startX = e.clientX;
    const startW = useUi.getState().sidebarWidth;
    const target = e.currentTarget;
    target.setPointerCapture(e.pointerId);
    setDragging(true);
    const onMove = (ev: PointerEvent) => setWidth(startW + ev.clientX - startX);
    const onUp = () => {
      setDragging(false);
      target.removeEventListener("pointermove", onMove);
      target.removeEventListener("pointerup", onUp);
      target.removeEventListener("pointercancel", onUp);
    };
    target.addEventListener("pointermove", onMove);
    target.addEventListener("pointerup", onUp);
    target.addEventListener("pointercancel", onUp);
  };

  return (
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label="Resize sidebar"
      onPointerDown={onPointerDown}
      onDoubleClick={() => setWidth(SIDEBAR_DEFAULT)}
      className={cn(
        "absolute -right-[3px] top-0 z-10 h-full w-[6px] cursor-col-resize transition-colors",
        dragging ? "bg-accent/40" : "hover:bg-line-strong",
      )}
    />
  );
}
