import { Braces, FileDown, Pencil, Star, StarOff, Trash2 } from "lucide-react";
import type { ExportFormat } from "@/bindings";
import { MenuItem, MenuSeparator } from "@/components/ui/Menu";

/** Star / Rename / Export / Delete items shared by the sidebar row menu and the title bar. */
export function ChatMenuItems({
  starred,
  onStar,
  onRename,
  onExport,
  onDelete,
}: {
  starred: boolean;
  onStar: () => void;
  onRename: () => void;
  onExport?: (format: ExportFormat) => void;
  onDelete: () => void;
}) {
  return (
    <>
      <MenuItem onSelect={onStar}>
        {starred ? <StarOff /> : <Star />}
        {starred ? "Unstar" : "Star"}
      </MenuItem>
      <MenuItem onSelect={onRename}>
        <Pencil />
        Rename
      </MenuItem>
      {onExport && (
        <>
          <MenuSeparator />
          <MenuItem onSelect={() => onExport("markdown")}>
            <FileDown />
            Export as Markdown
          </MenuItem>
          <MenuItem onSelect={() => onExport("json")}>
            <Braces />
            Export as JSON
          </MenuItem>
        </>
      )}
      <MenuSeparator />
      <MenuItem danger onSelect={onDelete}>
        <Trash2 />
        Delete
      </MenuItem>
    </>
  );
}
