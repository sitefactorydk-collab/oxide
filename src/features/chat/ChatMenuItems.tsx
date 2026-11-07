import {
  Braces,
  Check,
  FileDown,
  FolderInput,
  FolderMinus,
  Pencil,
  Star,
  StarOff,
  Trash2,
} from "lucide-react";
import type { Conversation, ExportFormat } from "@/bindings";
import {
  MenuItem,
  MenuSeparator,
  MenuSub,
  MenuSubContent,
  MenuSubTrigger,
} from "@/components/ui/Menu";
import { useProjects, useSetConversationProject } from "@/features/projects/queries";
import { errorMessage } from "@/lib/api";
import { toast } from "@/lib/toast";

/** Star / Rename / Move / Export / Delete items shared by the sidebar row menu and the title bar. */
export function ChatMenuItems({
  starred,
  onStar,
  onRename,
  onExport,
  onDelete,
  conversation,
}: {
  starred: boolean;
  onStar: () => void;
  onRename: () => void;
  onExport?: (format: ExportFormat) => void;
  onDelete: () => void;
  /** Shows "Move to project" for this chat. */
  conversation?: Conversation;
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
      {conversation && <MoveToProject conversation={conversation} />}
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

function MoveToProject({ conversation }: { conversation: Conversation }) {
  const { data: projects } = useProjects();
  const move = useSetConversationProject();
  const current = conversation.projectId;
  const choices = projects.filter((p) => !p.archived || p.id === current);
  // The menu (and this component) closes on select, so per-call mutate callbacks would never
  // run; the promise from mutateAsync still settles.
  const moveTo = (projectId: string | null, label: string) => {
    move
      .mutateAsync({ id: conversation.id, projectId })
      .then(() => toast(projectId ? `Moved to ${label}` : "Removed from the project"))
      .catch((e: unknown) => toast(errorMessage(e), "error"));
  };
  return (
    <MenuSub>
      <MenuSubTrigger>
        <FolderInput />
        Move to project
      </MenuSubTrigger>
      <MenuSubContent>
        {choices.length === 0 && <MenuItem disabled>No projects yet</MenuItem>}
        {choices.map((p) => (
          <MenuItem key={p.id} disabled={p.id === current} onSelect={() => moveTo(p.id, p.name)}>
            {p.id === current ? <Check /> : <span className="size-4" />}
            <span className="truncate">{p.name}</span>
          </MenuItem>
        ))}
        {current && (
          <>
            <MenuSeparator />
            <MenuItem onSelect={() => moveTo(null, "")}>
              <FolderMinus />
              Remove from project
            </MenuItem>
          </>
        )}
      </MenuSubContent>
    </MenuSub>
  );
}
