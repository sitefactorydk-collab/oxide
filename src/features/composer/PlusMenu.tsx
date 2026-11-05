import { Globe, Paperclip, Plug, Plus, Settings2 } from "lucide-react";
import { useState } from "react";
import type { AttachmentLimits } from "@/bindings";
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
import { formatBytes } from "@/features/attachments/limits";
import { useOpenSettings } from "@/features/settings/useOpenSettings";

const MOCK_CONNECTORS = ["filesystem", "github", "everything"];

/**
 * Composer "+" menu: attach files, and per-chat tool toggles (web search and connectors are
 * placeholders until Phases 5–6).
 */
export function PlusMenu({
  onPickFiles,
  limits,
}: {
  onPickFiles: () => void;
  limits: AttachmentLimits;
}) {
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
      <MenuContent align="start" side="top" className="w-80">
        <MenuItem onSelect={onPickFiles}>
          <Paperclip />
          <span className="flex min-w-0 flex-col">
            <span>Add files or photos</span>
            <span className="text-xs text-fg-subtle">
              Up to {limits.maxFiles} · images {formatBytes(limits.maxImageBytes)} · PDFs{" "}
              {formatBytes(limits.maxPdfBytes)}, {limits.maxPdfPages} pages · text{" "}
              {formatBytes(limits.maxTextBytes)}
            </span>
          </span>
        </MenuItem>
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
