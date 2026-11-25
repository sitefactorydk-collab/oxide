import { cn } from "@/lib/cn";
import { editCounts, editDiffRows, type FileEdits } from "./editBlocks";

/** Edit blocks as a compact diff: removed lines red, added lines green (Phase 26). */
export function EditDiff({ edits, className }: { edits: FileEdits; className?: string }) {
  const rows = editDiffRows(edits.blocks);
  const { added, removed } = editCounts(edits.blocks);
  const label = `Changes to ${edits.path || "the file"}`;
  return (
    <div className={cn("flex flex-col gap-1", className)}>
      <div className="flex items-center gap-2 text-xs text-fg-subtle">
        <span className="min-w-0 truncate font-mono" title={edits.path}>
          {edits.path}
        </span>
        <span className="shrink-0 text-success">+{added}</span>
        <span className="shrink-0 text-danger">−{removed}</span>
      </div>
      <pre
        aria-label={label}
        className="max-h-72 overflow-auto rounded-lg bg-sunken py-2 font-mono text-xs leading-5"
      >
        {rows.map((r, i) => (
          <div
            key={i}
            data-diff={r.kind}
            className={cn(
              "whitespace-pre-wrap break-words px-3",
              r.kind === "hunk"
                ? "mt-1 text-fg-subtle first:mt-0"
                : r.kind === "add"
                  ? "bg-success/10 text-success"
                  : r.kind === "remove"
                    ? "bg-danger-soft text-danger"
                    : "text-fg-muted",
            )}
          >
            {r.kind === "hunk"
              ? `@@ ${r.text} @@`
              : `${r.kind === "add" ? "+" : r.kind === "remove" ? "-" : " "}${r.text || " "}`}
          </div>
        ))}
      </pre>
    </div>
  );
}
