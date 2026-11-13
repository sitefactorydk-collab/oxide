import { AlertTriangle, FileText, Sparkles } from "lucide-react";
import type { SkillFileChange, SkillSavePreview } from "@/bindings";
import { cn } from "@/lib/cn";

/** A unified diff with removed lines red and added lines green. */
export function DiffView({ diff, label }: { diff: string; label: string }) {
  const lines = diff.split("\n").filter((l, i, all) => !(l === "" && i === all.length - 1));
  return (
    <pre
      aria-label={label}
      className="max-h-80 overflow-auto rounded-lg bg-sunken py-2 font-mono text-xs leading-5"
    >
      {lines.map((l, i) => (
        <div
          key={i}
          data-diff={
            l.startsWith("+++") || l.startsWith("---")
              ? "file"
              : l.startsWith("@@")
                ? "hunk"
                : l.startsWith("+")
                  ? "add"
                  : l.startsWith("-")
                    ? "remove"
                    : "same"
          }
          className={cn(
            "whitespace-pre-wrap break-words px-3",
            l.startsWith("+++") || l.startsWith("---")
              ? "text-fg-subtle"
              : l.startsWith("@@")
                ? "text-fg-subtle"
                : l.startsWith("+")
                  ? "bg-success/10 text-success"
                  : l.startsWith("-")
                    ? "bg-danger-soft text-danger"
                    : "text-fg-muted",
          )}
        >
          {l === "" ? " " : l}
        </div>
      ))}
    </pre>
  );
}

function sizeText(n: number): string {
  return n < 1024 ? `${n} B` : `${Math.round(n / 1024)} KB`;
}

function FileChange({ file }: { file: SkillFileChange }) {
  return (
    <details className="rounded-lg border border-line">
      <summary className="flex cursor-pointer items-center gap-2 px-3 py-1.5 text-sm text-fg">
        <FileText aria-hidden className="size-3.5 shrink-0 text-fg-muted" />
        <span className="truncate font-mono">{file.path}</span>
        <span className="ml-auto shrink-0 text-fg-subtle">
          {file.isNew ? "new" : file.diff ? "changed" : "unchanged"} · {sizeText(file.bytes)}
        </span>
      </summary>
      <div className="px-3 pb-3">
        {file.diff ? (
          <DiffView diff={file.diff} label={`Changes to ${file.path}`} />
        ) : (
          <pre
            aria-label={`Content of ${file.path}`}
            className="max-h-60 overflow-auto whitespace-pre-wrap break-words rounded-lg bg-sunken px-3 py-2 font-mono text-xs leading-5 text-fg-muted"
          >
            {file.content}
          </pre>
        )}
      </div>
    </details>
  );
}

/** What a `save_skill` call would write: notes, the diff of an edit, the whole SKILL.md, files. */
export function SkillSaveDetails({ preview }: { preview: SkillSavePreview }) {
  return (
    <div className="flex flex-col gap-2.5">
      <p className="flex items-center gap-1.5 text-sm text-fg-muted">
        <Sparkles aria-hidden className="size-3.5 shrink-0 text-accent" />
        {preview.isNew
          ? "A new skill in your skills folder. Later chats can load it; you can edit, switch off or delete it in Settings → Skills."
          : "This changes the skill for every chat that uses it."}
      </p>
      {preview.notes.map((n) => (
        <p key={n} className="flex items-start gap-1.5 text-sm text-danger">
          <AlertTriangle aria-hidden className="mt-0.5 size-3.5 shrink-0" />
          {n}
        </p>
      ))}
      {!preview.isNew &&
        (preview.diff ? (
          <DiffView diff={preview.diff} label="Changes to SKILL.md" />
        ) : (
          <p className="text-sm text-fg-subtle">SKILL.md stays the same.</p>
        ))}
      <details open={preview.isNew} className="rounded-lg border border-line">
        <summary className="cursor-pointer px-3 py-1.5 text-sm text-fg">
          {preview.isNew ? "SKILL.md" : "The whole new SKILL.md"}
        </summary>
        <div className="px-3 pb-3">
          <pre
            aria-label="SKILL.md"
            className="max-h-80 overflow-auto whitespace-pre-wrap break-words rounded-lg bg-sunken px-3 py-2 font-mono text-xs leading-5 text-fg"
          >
            {preview.skillMd}
          </pre>
        </div>
      </details>
      {preview.files.length > 0 && (
        <div className="flex flex-col gap-1.5" aria-label="Files">
          <p className="text-sm text-fg-muted">
            {preview.files.length === 1 ? "1 more file" : `${preview.files.length} more files`} in
            the skill’s folder:
          </p>
          {preview.files.map((f) => (
            <FileChange key={f.path} file={f} />
          ))}
        </div>
      )}
    </div>
  );
}
