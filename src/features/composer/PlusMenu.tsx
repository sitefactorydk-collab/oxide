import { FolderClosed, Globe, Paperclip, Plug, Plus, Settings2 } from "lucide-react";
import { useState } from "react";
import { IconButton } from "@/components/ui/IconButton";
import {
  MenuCheckboxItem,
  MenuContent,
  MenuItem,
  MenuRoot,
  MenuSeparator,
  MenuSub,
  MenuSubContent,
  MenuSubTrigger,
  MenuTrigger,
} from "@/components/ui/Menu";
import { Tooltip } from "@/components/ui/Tooltip";
import { useMock } from "@/features/mock/store";
import { useOpenSettings } from "@/features/settings/useOpenSettings";

const MOCK_CONNECTORS = ["filesystem", "github", "everything"];

/** Composer "+" menu: attach, add from project, and per-chat tool toggles (UI only in Phase 1). */
export function PlusMenu({
  onPickFiles,
  onAddProjectContext,
}: {
  onPickFiles: () => void;
  onAddProjectContext: (label: string) => void;
}) {
  const projects = useMock((s) => s.projects);
  const openSettings = useOpenSettings();
  const [webSearch, setWebSearch] = useState(true);
  const [enabled, setEnabled] = useState<Record<string, boolean>>({ filesystem: true });

  return (
    <MenuRoot>
      <Tooltip label="Add files, context and tools">
        <MenuTrigger asChild>
          <IconButton label="Add files, context and tools" tooltip={false}>
            <Plus className="size-[18px]" />
          </IconButton>
        </MenuTrigger>
      </Tooltip>
      <MenuContent align="start" side="top" className="w-64">
        <MenuItem onSelect={onPickFiles}>
          <Paperclip />
          Add files or photos
        </MenuItem>
        <MenuSub>
          <MenuSubTrigger>
            <FolderClosed />
            Add from a project
          </MenuSubTrigger>
          <MenuSubContent>
            {projects.map((p) => (
              <MenuItem key={p.id} onSelect={() => onAddProjectContext(`${p.name} knowledge`)}>
                <FolderClosed />
                {p.name}
              </MenuItem>
            ))}
          </MenuSubContent>
        </MenuSub>
        <MenuSeparator />
        <MenuCheckboxItem
          checked={webSearch}
          onCheckedChange={(v) => setWebSearch(v === true)}
          onSelect={(e) => e.preventDefault()}
        >
          <Globe />
          Web search
        </MenuCheckboxItem>
        <MenuSub>
          <MenuSubTrigger>
            <Plug />
            Connectors
          </MenuSubTrigger>
          <MenuSubContent>
            {MOCK_CONNECTORS.map((name) => (
              <MenuCheckboxItem
                key={name}
                checked={enabled[name] === true}
                onCheckedChange={(v) => setEnabled((s) => ({ ...s, [name]: v === true }))}
                onSelect={(e) => e.preventDefault()}
              >
                <span className="font-mono text-sm">{name}</span>
              </MenuCheckboxItem>
            ))}
            <MenuSeparator />
            <MenuItem onSelect={() => openSettings("connectors")}>
              <Settings2 />
              Manage connectors
            </MenuItem>
          </MenuSubContent>
        </MenuSub>
      </MenuContent>
    </MenuRoot>
  );
}
