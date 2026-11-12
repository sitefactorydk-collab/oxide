import { ArrowUp, Upload } from "lucide-react";
import {
  useLayoutEffect,
  useMemo,
  useRef,
  type ClipboardEvent,
  type KeyboardEvent as ReactKeyboardEvent,
} from "react";
import { createPortal } from "react-dom";
import { commands, type Attachment } from "@/bindings";
import { IconButton } from "@/components/ui/IconButton";
import { Tooltip } from "@/components/ui/Tooltip";
import { AttachmentChips } from "@/features/attachments/AttachmentChips";
import { limitsSummary } from "@/features/attachments/limits";
import { useAttachments } from "@/features/attachments/useAttachments";
import { ContextMeter } from "@/features/context/ContextMeter";
import { findListing, findModel } from "@/features/models/models";
import { useModels } from "@/features/models/queries";
import { isExtensionFile, useExtensionDrop } from "@/features/extensions/store";
import { useOpenSettings } from "@/features/settings/useOpenSettings";
import { useFolderControls } from "@/features/workspace/folders";
import { FolderChips } from "@/features/workspace/FolderChips";
import { cn } from "@/lib/cn";
import { inTauri } from "@/lib/platform";
import { EffortMenu, ThinkingToggle } from "./ReasoningControls";
import { ModelPicker } from "./ModelPicker";
import { PlusMenu } from "./PlusMenu";
import { useFileDrop } from "./useFileDrop";

const MAX_HEIGHT = 320;

/**
 * The message composer.
 * - `home`: large box with the toolbar inside (new-chat screen).
 * - `reply`: compact box with the toolbar underneath (conversation view).
 *
 * Files can be added with the "+" menu (native picker), by dropping them anywhere on the
 * window, or by pasting (screenshots). They're prepared by the backend straight away; Send
 * waits until every file is ready.
 */
export function Composer({
  value,
  onChange,
  onSubmit,
  variant,
  modelId,
  onModelChange,
  streaming = false,
  onStop,
  placeholder,
  autoFocus,
  conversationId,
  leafId = null,
}: {
  value: string;
  onChange: (v: string) => void;
  onSubmit: (text: string, attachments: Attachment[]) => void;
  variant: "home" | "reply";
  /** Model the message goes to (`<providerId>/<model>`). */
  modelId: string;
  onModelChange: (id: string) => void;
  streaming?: boolean;
  onStop?: () => void;
  placeholder?: string;
  autoFocus?: boolean;
  /** The chat this composer writes in (`undefined` on the new chat screens). */
  conversationId?: string;
  /** The last message of the thread shown (for the context meter). */
  leafId?: string | null;
}) {
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const files = useAttachments();
  const folders = useFolderControls(conversationId);
  const { data: models } = useModels();
  const model = findModel(models, modelId);
  const providerKind = findListing(models, modelId)?.providerKind;

  const { addPaths, addFiles } = files;
  const openSettings = useOpenSettings();
  const dropHandlers = useMemo(
    () => ({
      onPaths: (paths: string[]) => {
        // A desktop extension opens its install dialog instead of being attached.
        const bundle = paths.find(isExtensionFile);
        if (bundle) {
          useExtensionDrop.getState().request(bundle);
          openSettings("extensions");
        }
        const rest = paths.filter((p) => !isExtensionFile(p));
        if (rest.length === 0) return;
        addPaths(rest);
        textareaRef.current?.focus();
      },
      onFiles: (list: File[]) => {
        addFiles(list);
        textareaRef.current?.focus();
      },
    }),
    [addPaths, addFiles, openSettings],
  );
  const dragging = useFileDrop(dropHandlers);

  useLayoutEffect(() => {
    const ta = textareaRef.current;
    if (!ta) return;
    ta.style.height = "auto";
    ta.style.height = `${Math.min(ta.scrollHeight, MAX_HEIGHT)}px`;
    ta.style.overflowY = ta.scrollHeight > MAX_HEIGHT ? "auto" : "hidden";
  }, [value]);

  const hasContent = value.trim().length > 0 || files.ready.length > 0;
  const canSend = hasContent && !files.busy;

  const submit = () => {
    if (!canSend || streaming) return;
    onSubmit(value.trim(), files.ready);
    files.clear();
  };

  const onKeyDown = (e: ReactKeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
      e.preventDefault();
      submit();
    }
  };

  const onPaste = (e: ClipboardEvent<HTMLTextAreaElement>) => {
    const pasted = Array.from(e.clipboardData?.files ?? []);
    if (pasted.length === 0) return;
    e.preventDefault();
    addFiles(pasted, true);
  };

  const pickFiles = () => {
    if (!inTauri) {
      fileInputRef.current?.click();
      return;
    }
    void commands.pickFiles().then((res) => {
      if (res.status === "ok" && res.data.length) addPaths(res.data);
      textareaRef.current?.focus();
    });
  };

  const sendLabel = files.busy ? "Preparing files…" : "Send";
  const textarea = (
    <textarea
      ref={textareaRef}
      aria-label="Message"
      aria-describedby="composer-hint"
      rows={variant === "home" ? 2 : 1}
      value={value}
      autoFocus={autoFocus}
      placeholder={placeholder ?? (variant === "home" ? "What should we work on?" : "Reply…")}
      onChange={(e) => onChange(e.target.value)}
      onKeyDown={onKeyDown}
      onPaste={onPaste}
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
    <Tooltip label={sendLabel} shortcut={files.busy ? undefined : "Enter"}>
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
        onPickFiles={pickFiles}
        onPickFolder={() => void folders.pick()}
        limits={files.limits}
        conversationId={conversationId}
        onAttach={files.addPrepared}
      />
      <ThinkingToggle model={model} />
      <EffortMenu model={model} />
    </>
  );

  return (
    <div className="w-full">
      <span id="composer-hint" className="sr-only">
        Enter sends, Shift+Enter adds a line. Paste or drop files to attach them.
      </span>
      <div
        className={cn(
          "rounded-2xl border border-line bg-raised shadow-composer transition-colors focus-within:border-line-strong",
          variant === "home" ? "px-3 pb-2.5 pt-3.5" : "px-3 py-2",
        )}
      >
        <FolderChips folders={folders.folders} onRemove={folders.remove} />
        {files.items.length > 0 && (
          <AttachmentChips
            items={files.items}
            onRemove={files.remove}
            caps={model?.capabilities}
            providerKind={providerKind}
          />
        )}
        {variant === "home" ? (
          <>
            {textarea}
            <div className="mt-2 flex items-center gap-1">
              {tools}
              <div className="flex-1" />
              <ModelPicker value={modelId} onChange={onModelChange} />
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
          {conversationId && (
            <ContextMeter
              conversationId={conversationId}
              leafId={leafId}
              modelId={modelId}
              streaming={streaming}
            />
          )}
          <ModelPicker value={modelId} onChange={onModelChange} />
        </div>
      )}

      <input
        ref={fileInputRef}
        type="file"
        multiple
        hidden
        aria-label="Add files"
        onChange={(e) => {
          const list = Array.from(e.target.files ?? []);
          if (list.length) addFiles(list);
          e.target.value = "";
          textareaRef.current?.focus();
        }}
      />

      {dragging && createPortal(<DropOverlay hint={limitsSummary(files.limits)} />, document.body)}
    </div>
  );
}

function DropOverlay({ hint }: { hint: string }) {
  return (
    <div className="pointer-events-none fixed inset-0 z-50 flex items-center justify-center bg-overlay p-8">
      <div className="ox-pop flex h-full max-h-80 w-full max-w-xl flex-col items-center justify-center gap-3 rounded-2xl border-2 border-dashed border-accent bg-raised/95 px-8 text-center shadow-lg">
        <span className="inline-flex size-12 items-center justify-center rounded-full bg-accent-soft text-accent">
          <Upload className="size-6" />
        </span>
        <p className="font-serif text-xl text-fg">Drop files to add them to the chat</p>
        <p className="text-sm text-fg-muted">Images, PDFs, text and code files</p>
        <p className="text-xs text-fg-subtle">{hint}</p>
      </div>
    </div>
  );
}
