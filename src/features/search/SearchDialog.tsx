import { MessageSquare, Search } from "lucide-react";
import { Dialog } from "radix-ui";
import { useMemo, useState, type ReactNode } from "react";
import { useNavigate } from "react-router";
import { useConversations, useSearch } from "@/features/chats/queries";
import { cn } from "@/lib/cn";
import { formatRelative } from "@/lib/time";
import { useDebounced } from "@/lib/useDebounced";
import { useUi } from "@/stores/ui";

interface Hit {
  id: string;
  title: string;
  snippet?: string;
  updatedAt: number;
}

/** Ctrl/Cmd+K: full-text search over chat titles and message text. */
export function SearchDialog() {
  const open = useUi((s) => s.searchOpen);
  const setOpen = useUi((s) => s.setSearchOpen);
  return (
    <Dialog.Root open={open} onOpenChange={setOpen}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-40 bg-overlay" />
        <Dialog.Content
          aria-describedby={undefined}
          className="ox-pop fixed left-1/2 top-[12vh] z-50 w-[min(640px,calc(100vw-48px))] -translate-x-1/2 overflow-hidden rounded-2xl border border-line bg-raised shadow-lg outline-none"
        >
          <Dialog.Title className="sr-only">Search chats</Dialog.Title>
          {/* Remount per open so the query resets. */}
          {open && <SearchBody onDone={() => setOpen(false)} />}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

const MARK_START = "\u0002";
const MARK_END = "\u0003";

/** Renders an FTS snippet, highlighting terms marked with U+0002 … U+0003. */
export function Snippet({ text }: { text: string }) {
  const parts: ReactNode[] = [];
  text.split(MARK_START).forEach((chunk, i) => {
    if (i === 0) {
      if (chunk) parts.push(chunk);
      return;
    }
    const end = chunk.indexOf(MARK_END);
    const marked = end >= 0 ? chunk.slice(0, end) : chunk;
    const rest = end >= 0 ? chunk.slice(end + MARK_END.length) : "";
    parts.push(
      <mark key={i} className="rounded-sm bg-accent-soft px-0.5 text-fg">
        {marked}
      </mark>,
    );
    if (rest) parts.push(rest);
  });
  return <>{parts}</>;
}

function SearchBody({ onDone }: { onDone: () => void }) {
  const { data: conversations } = useConversations();
  const navigate = useNavigate();
  const [q, setQ] = useState("");
  const [active, setActive] = useState(0);
  const query = useDebounced(q.trim(), 120);
  const search = useSearch(query);

  const hits = useMemo<Hit[]>(() => {
    if (!query) {
      return conversations
        .slice(0, 8)
        .map((c) => ({ id: c.id, title: c.title, updatedAt: c.updatedAt }));
    }
    return (search.data ?? []).map((h) => ({
      id: h.conversation.id,
      title: h.conversation.title,
      updatedAt: h.conversation.updatedAt,
      ...(h.snippet ? { snippet: h.snippet } : {}),
    }));
  }, [query, conversations, search.data]);

  const go = (hit: Hit | undefined) => {
    if (!hit) return;
    onDone();
    navigate(`/chat/${hit.id}`);
  };

  return (
    <>
      <div className="flex items-center gap-3 border-b border-line px-4">
        <Search className="size-[18px] shrink-0 text-fg-subtle" />
        <input
          autoFocus
          aria-label="Search chats"
          value={q}
          onChange={(e) => {
            setQ(e.target.value);
            setActive(0);
          }}
          onKeyDown={(e) => {
            if (e.key === "ArrowDown") {
              e.preventDefault();
              setActive((i) => Math.min(hits.length - 1, i + 1));
            } else if (e.key === "ArrowUp") {
              e.preventDefault();
              setActive((i) => Math.max(0, i - 1));
            } else if (e.key === "Enter") {
              e.preventDefault();
              go(hits[active]);
            }
          }}
          placeholder="Search chats…"
          className="h-14 flex-1 bg-transparent text-lg text-fg outline-none"
        />
      </div>
      <ul role="listbox" aria-label="Results" className="max-h-[50vh] overflow-y-auto p-2">
        {!q.trim() && hits.length > 0 && (
          <li className="px-2.5 pb-1 pt-1 text-xs font-medium text-fg-subtle">Recent</li>
        )}
        {hits.map((h, i) => (
          <li key={h.id} role="option" aria-selected={i === active}>
            <button
              type="button"
              onMouseMove={() => setActive(i)}
              onClick={() => go(h)}
              className={cn(
                "flex w-full items-start gap-3 rounded-lg px-2.5 py-2 text-left",
                i === active && "bg-hover",
              )}
            >
              <MessageSquare className="mt-0.5 size-4 shrink-0 text-fg-subtle" />
              <span className="min-w-0 flex-1">
                <span className="block truncate text-md text-fg">{h.title}</span>
                {h.snippet && (
                  <span className="block truncate text-sm text-fg-muted">
                    <Snippet text={h.snippet} />
                  </span>
                )}
              </span>
              <span className="shrink-0 text-xs text-fg-subtle">{formatRelative(h.updatedAt)}</span>
            </button>
          </li>
        ))}
        {hits.length === 0 && (query ? !search.isFetching : true) && (
          <li className="px-3 py-8 text-center text-fg-muted">
            {query ? `No chats match “${q.trim()}”` : "No chats yet"}
          </li>
        )}
      </ul>
    </>
  );
}
