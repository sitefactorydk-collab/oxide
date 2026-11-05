import {
  ArrowUp,
  FileText,
  FolderClosed,
  Image as ImageIcon,
  Lightbulb,
  Upload,
  X,
} from "lucide-react";
import {
  useCallback,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
} from "react";
import { createPortal } from "react-dom";
import { IconButton } from "@/components/ui/IconButton";
import { Tooltip } from "@/components/ui/Tooltip";
import { cn } from "@/lib/cn";
import { useUi } from "@/stores/ui";
import { ModelPicker } from "./ModelPicker";
import { PlusMenu } from "./PlusMenu";
import { useFileDrop } from "./useFileDrop";

export interface Attachment {
  id: string;
  name: string;
  kind: "image" | "pdf" | "file" | "context";
}

const MAX_HEIGHT = 320;
let attachSeq = 0;

function kindOf(name: string): Attachment["kind"] {
  const ext = name.split(".").pop()?.toLowerCase() ?? "";
  if (["png", "jpg", "jpeg", "gif", "webp", "bmp"].includes(ext)) return "image";
  if (ext === "pdf") return "pdf";
  return "file";
}

/**
 * The message composer.
 * - `home`: large box with the toolbar inside (new-chat screen).
 * - `reply`: compact box with the toolbar underneath (conversation view).
 */
export function Composer({
  value,
  onChange,
  onSubmit,
  variant,
  streaming = false,
  onStop,
  placeholder,
  autoFocus,
}: {
  value: string;
  onChange: (v: string) => void;
  onSubmit: (text: string, attachments: Attachment[]) => void;
  variant: "home" | "reply";
  streaming?: boolean;
  onStop?: () => void;
  placeholder?: string;
  autoFocus?: boolean;
}) {
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const [attachments, setAttachments] = useState<Attachment[]>([]);

  const addNames = useCallback((names: string[], kind?: Attachment["kind"]) => {
    setAttachments((prev) => [
      ...prev,
      ...names.map((name) => {
        attachSeq += 1;
        return { id: `att${attachSeq}`, name, kind: kind ?? kindOf(name) };
      }),
    ]);
    textareaRef.current?.focus();
  }, []);

  const dragging = useFileDrop(addNames);

  useLayoutEffect(() => {
    const ta = textareaRef.current;
    if (!ta) return;
    ta.style.height = "auto";
    ta.style.height = `${Math.min(ta.scrollHeight, MAX_HEIGHT)}px`;
    ta.style.overflowY = ta.scrollHeight > MAX_HEIGHT ? "auto" : "hidden";
  }, [value]);

  const canSend = value.trim().length > 0 || attachments.length > 0;

  const submit = () => {
    if (!canSend || streaming) return;
    onSubmit(value.trim(), attachments);
    setAttachments([]);
  };

  const onKeyDown = (e: ReactKeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
      e.preventDefault();
      submit();
    }
  };

  const textarea = (
    <textarea
      ref={textareaRef}
      aria-label="Message"
      rows={variant === "home" ? 2 : 1}
      value={value}
      autoFocus={autoFocus}
      placeholder={placeholder ?? (variant === "home" ? "What should we work on?" : "Reply…")}
      onChange={(e) => onChange(e.target.value)}
      onKeyDown={onKeyDown}
      className={cn(
        "block w-full resize-none bg-transparent text-md leading-6 text-fg outline-none",
        variant === "home" ? "min-h-12 px-1" : "min-h-6 py-1",
      )}
    />
  );

  const sendButton = streaming ? (
    <IconButton label="Stop" onClick={onStop} className="text-fg">
      <span className="size-3 rounded-[3px] bg-current" />
    </IconButton>
  ) : (
    <Tooltip label="Send" shortcut="Enter">
      <button
        type="button"
        aria-label="Send"
        disabled={!canSend}
        onClick={submit}
        className="inline-flex size-8 shrink-0 items-center justify-center rounded-lg bg-accent text-accent-fg transition-colors hover:bg-accent-hover disabled:opacity-35"
      >
        <ArrowUp className="size-[18px]" strokeWidth={2.25} />
      </button>
    </Tooltip>
  );

  const tools = (
    <>
      <PlusMenu
        onPickFiles={() => fileInputRef.current?.click()}
        onAddProjectContext={(label) => addNames([label], "context")}
      />
      <ThinkingToggle />
    </>
  );

  return (
    <div className="w-full">
      <div
        className={cn(
          "rounded-2xl border border-line bg-raised shadow-composer transition-colors focus-within:border-line-strong",
          variant === "home" ? "px-3 pb-2.5 pt-3.5" : "px-3 py-2",
        )}
      >
        {attachments.length > 0 && (
          <AttachmentChips
            items={attachments}
            onRemove={(id) => setAttachments((a) => a.filter((x) => x.id !== id))}
          />
        )}
        {variant === "home" ? (
          <>
            {textarea}
            <div className="mt-2 flex items-center gap-1">
              {tools}
              <div className="flex-1" />
              <ModelPicker />
              {sendButton}
            </div>
          </>
        ) : (
          <div className="flex items-end gap-2 pl-1">
            <div className="min-w-0 flex-1">{textarea}</div>
            {sendButton}
          </div>
        )}
      </div>

      {variant === "reply" && (
        <div className="mt-1.5 flex items-center gap-1 px-1">
          {tools}
          <p className="flex-1 truncate px-2 text-center text-xs text-fg-subtle">
            AI can be wrong. Check anything important.
          </p>
          <ModelPicker />
        </div>
      )}

      <input
        ref={fileInputRef}
        type="file"
        multiple
        hidden
        onChange={(e) => {
          const names = Array.from(e.target.files ?? []).map((f) => f.name);
          if (names.length) addNames(names);
          e.target.value = "";
        }}
      />

      {dragging && createPortal(<DropOverlay />, document.body)}
    </div>
  );
}

function ThinkingToggle() {
  const thinking = useUi((s) => s.thinking);
  const toggle = useUi((s) => s.toggleThinking);
  return (
    <Tooltip label={thinking ? "Extended thinking on" : "Extended thinking off"}>
      <button
        type="button"
        aria-pressed={thinking}
        aria-label="Extended thinking"
        onClick={toggle}
        className={cn(
          "inline-flex h-8 items-center gap-1.5 rounded-lg px-2 text-sm transition-colors",
          thinking ? "bg-accent-soft text-accent" : "text-fg-muted hover:bg-hover hover:text-fg",
        )}
      >
        <Lightbulb className="size-[18px]" />
        {thinking && <span className="font-medium">Thinking</span>}
      </button>
    </Tooltip>
  );
}

function AttachmentChips({
  items,
  onRemove,
}: {
  items: Attachment[];
  onRemove: (id: string) => void;
}) {
  return (
    <ul aria-label="Attachments" className="mb-2 flex flex-wrap gap-2">
      {items.map((a) => {
        const Icon =
          a.kind === "image" ? ImageIcon : a.kind === "context" ? FolderClosed : FileText;
        return (
          <li
            key={a.id}
            className="group flex h-9 max-w-56 items-center gap-2 rounded-lg border border-line bg-sunken pl-2.5 pr-1 text-sm"
          >
            <Icon className="size-4 shrink-0 text-fg-muted" />
            <span className="truncate">{a.name}</span>
            <button
              type="button"
              aria-label={`Remove ${a.name}`}
              onClick={() => onRemove(a.id)}
              className="inline-flex size-6 shrink-0 items-center justify-center rounded-md text-fg-subtle hover:bg-hover hover:text-fg"
            >
              <X className="size-3.5" />
            </button>
          </li>
        );
      })}
    </ul>
  );
}

function DropOverlay() {
  return (
    <div className="pointer-events-none fixed inset-0 z-50 flex items-center justify-center bg-overlay p-8">
      <div className="ox-pop flex h-full max-h-80 w-full max-w-xl flex-col items-center justify-center gap-3 rounded-2xl border-2 border-dashed border-accent bg-raised/95 text-center shadow-lg">
        <span className="inline-flex size-12 items-center justify-center rounded-full bg-accent-soft text-accent">
          <Upload className="size-6" />
        </span>
        <p className="font-serif text-xl text-fg">Drop files to add them to the chat</p>
        <p className="text-sm text-fg-muted">Images, PDFs, text and code files</p>
      </div>
    </div>
  );
}
