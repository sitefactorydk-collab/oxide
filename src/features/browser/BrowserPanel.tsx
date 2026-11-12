import { Compass, Square } from "lucide-react";
import type { ReactNode } from "react";
import { useOpenSettings } from "@/features/settings/useOpenSettings";
import { errorMessage } from "@/lib/api";
import { toast } from "@/lib/toast";
import { useBrowserLive, useBrowserStatus, useCloseBrowser } from "./queries";

/**
 * The chat's browser in the session panel: open or closed, what it shows last, and Close.
 * Shown when the browser is on in Settings.
 */
export function BrowserPanelSection({
  id,
  wrap,
}: {
  id: string;
  wrap: (title: string, aside: ReactNode, body: ReactNode) => ReactNode;
}) {
  const { data: status } = useBrowserStatus();
  const close = useCloseBrowser();
  const openSettings = useOpenSettings();
  // The chat's latest screenshot (from any of its calls this session).
  const last = useBrowserLive((s) => {
    const mine = Object.values(s.frames).filter((f) => f.conversationId === id);
    return mine[mine.length - 1];
  });
  const open = status?.openChats.includes(id) ?? false;
  const aside = (
    <span className="rounded-full bg-sunken px-2 py-0.5 text-xs text-fg-subtle">
      {open ? "Open" : "Closed"}
    </span>
  );
  const body = (
    <div className="flex flex-col gap-2">
      {open && last?.url && (
        <p className="truncate text-sm text-fg-muted" title={last.url}>
          <Compass aria-hidden className="mr-1 inline size-3.5" />
          {last.url}
        </p>
      )}
      {open && last?.image && (
        <img
          alt="The chat's browser"
          src={`data:image/jpeg;base64,${last.image}`}
          className="w-full rounded-lg border border-line"
        />
      )}
      {!open && (
        <p className="text-sm text-fg-subtle">
          {status?.installing
            ? "Setting up the browser…"
            : "Opens when the model uses the browser, and closes when idle."}
        </p>
      )}
      <div className="flex flex-wrap gap-2">
        {open && (
          <button
            type="button"
            disabled={close.isPending}
            onClick={() => close.mutate(id, { onError: (e) => toast(errorMessage(e), "error") })}
            className="inline-flex h-7 items-center gap-1.5 rounded-lg border border-line px-2 text-sm text-fg hover:bg-hover disabled:opacity-50"
          >
            <Square className="size-3.5" />
            Close the browser
          </button>
        )}
        <button
          type="button"
          onClick={() => openSettings("browser")}
          className="inline-flex h-7 items-center rounded-lg px-2 text-sm text-fg-muted hover:bg-hover hover:text-fg"
        >
          Browser settings
        </button>
      </div>
    </div>
  );
  return wrap("Browser", aside, body);
}
