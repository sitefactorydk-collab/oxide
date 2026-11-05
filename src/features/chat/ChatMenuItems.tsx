import { Pencil, Star, StarOff, Trash2 } from "lucide-react";
import { MenuItem, MenuSeparator } from "@/components/ui/Menu";

/** Star / Rename / Delete items shared by the sidebar row menu and the title-bar menu. */
export function ChatMenuItems({
  starred,
  onStar,
  onRename,
  onDelete,
}: {
  starred: boolean;
  onStar: () => void;
  onRename: () => void;
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
      <MenuSeparator />
      <MenuItem danger onSelect={onDelete}>
        <Trash2 />
        Delete
      </MenuItem>
    </>
  );
}
