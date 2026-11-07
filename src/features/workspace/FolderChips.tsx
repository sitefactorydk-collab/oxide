import { Folder, X } from "lucide-react";
import { Tooltip } from "@/components/ui/Tooltip";
import type { FolderView } from "./folders";

/** The composer's chips for folders shared with the chat. */
export function FolderChips({
  folders,
  onRemove,
}: {
  folders: FolderView[];
  onRemove: (path: string) => void;
}) {
  if (folders.length === 0) return null;
  return (
    <ul aria-label="Shared folders" className="mb-2 flex flex-wrap gap-2">
      {folders.map((f) => (
        <li
          key={f.path}
          className="flex h-8 max-w-64 items-center gap-1.5 rounded-lg border border-line bg-sunken pl-2 pr-1 text-sm text-fg"
        >
          <Folder aria-hidden className="size-4 shrink-0 text-fg-subtle" />
          <Tooltip label={f.path}>
            <span className="truncate">{f.name}</span>
          </Tooltip>
          <button
            type="button"
            aria-label={`Stop sharing ${f.name}`}
            onClick={() => onRemove(f.path)}
            className="inline-flex size-6 shrink-0 items-center justify-center rounded-md text-fg-subtle hover:bg-hover hover:text-fg"
          >
            <X className="size-3.5" />
          </button>
        </li>
      ))}
    </ul>
  );
}
