import { ArrowUp, Camera, LoaderCircle, X } from "lucide-react";
import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { commands, type Attachment } from "@/bindings";
import { OxideMark } from "@/components/OxideMark";
import { IconButton } from "@/components/ui/IconButton";
import { useStartConversation } from "@/features/chats/queries";
import { useGenerateOptions } from "@/features/composer/options";
import { useModelLabel, useNewChatModel } from "@/features/models/queries";
import { errorMessage, unwrap } from "@/lib/api";
import { cn } from "@/lib/cn";
import { onWindowFocusChanged } from "@/lib/window";
import { useUi } from "@/stores/ui";

/**
 * The quick entry window: one line to ask something from any app, an optional screenshot,
 * and Enter to start the chat in Oxide's main window. Esc or clicking elsewhere hides it.
 */
export function QuickEntry() {
  const [modelId] = useNewChatModel();
  const modelLabel = useModelLabel(modelId);
  const options = useGenerateOptions();
  const start = useStartConversation();
  const [text, setText] = useState("");
  const [shot, setShot] = useState<Attachment | null>(null);
  const [capturing, setCapturing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    let stop: (() => void) | undefined;
    let disposed = false;
    onWindowFocusChanged((focused) => {
      if (!focused) return;
      // The main window may have changed the model or theme since this window last showed.
      void useUi.persist.rehydrate();
      inputRef.current?.focus();
    })
      .then((unlisten) => (disposed ? unlisten() : (stop = unlisten)))
      .catch(() => {});
    return () => {
      disposed = true;
      stop?.();
    };
  }, []);

  useEffect(() => {
    const onKey = (e: globalThis.KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        void commands.hideQuickEntry();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const busy = start.isPending;
  const canSend = (text.trim() !== "" || shot !== null) && !busy && !capturing;

  const send = async () => {
    if (!canSend) return;
    setError(null);
    try {
      const chat = await start.mutateAsync({
        input: {
          text: text.trim(),
          model: modelId,
          projectId: null,
          attachments: shot ? [shot] : [],
        },
        options,
      });
      setText("");
      setShot(null);
      await commands.openQuickEntryChat(chat.id);
    } catch (e) {
      setError(`Couldn’t start the chat: ${errorMessage(e)}`);
    }
  };

  const capture = async () => {
    setCapturing(true);
    setError(null);
    try {
      setShot(await unwrap(commands.captureScreenshot()));
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setCapturing(false);
      inputRef.current?.focus();
    }
  };

  const onKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
      e.preventDefault();
      void send();
    }
  };

  return (
    <form
      aria-label="Quick entry"
      onSubmit={(e) => {
        e.preventDefault();
        void send();
      }}
      className="flex h-full flex-col overflow-hidden border border-line-strong bg-app text-fg"
    >
      <textarea
        ref={inputRef}
        aria-label="Ask Oxide"
        autoFocus
        rows={2}
        value={text}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={onKeyDown}
        placeholder="How can I help you today?"
        className="min-h-0 flex-1 resize-none bg-transparent px-4 pt-3.5 font-serif text-xl leading-snug text-fg outline-none placeholder:text-fg-subtle"
      />
      <div data-tauri-drag-region className="flex h-12 shrink-0 items-center gap-2 px-3 pb-1">
        <OxideMark className="pointer-events-none size-5 shrink-0" />
        {shot && <ShotChip shot={shot} onRemove={() => setShot(null)} />}
        <span
          role={error ? "alert" : undefined}
          className={cn(
            "pointer-events-none min-w-0 flex-1 truncate text-sm",
            error ? "text-danger" : "text-fg-subtle",
          )}
        >
          {error ?? (shot ? "" : `${modelLabel} · Enter to send, Esc to close`)}
        </span>
        <IconButton
          label={shot ? "Retake screenshot" : "Attach a screenshot"}
          tooltipSide="top"
          onClick={() => void capture()}
          disabled={capturing || busy}
        >
          {capturing ? (
            <LoaderCircle className="size-[18px] animate-spin" />
          ) : (
            <Camera className="size-[18px]" />
          )}
        </IconButton>
        <button
          type="submit"
          aria-label="Send"
          disabled={!canSend}
          className="inline-flex size-8 items-center justify-center rounded-lg bg-accent text-accent-fg transition-colors hover:bg-accent-hover disabled:opacity-40"
        >
          {busy ? <LoaderCircle className="size-4 animate-spin" /> : <ArrowUp className="size-4" />}
        </button>
      </div>
    </form>
  );
}

function ShotChip({ shot, onRemove }: { shot: Attachment; onRemove: () => void }) {
  return (
    <span className="inline-flex h-8 shrink-0 items-center gap-2 rounded-lg border border-line bg-raised pl-1 pr-1 text-sm text-fg">
      {shot.thumbnail ? (
        <img src={shot.thumbnail} alt="" className="h-6 w-9 rounded object-cover" />
      ) : (
        <Camera className="size-4 text-fg-subtle" />
      )}
      Screenshot
      <IconButton label="Remove screenshot" size="sm" tooltip={false} onClick={onRemove}>
        <X className="size-3.5" />
      </IconButton>
    </span>
  );
}
