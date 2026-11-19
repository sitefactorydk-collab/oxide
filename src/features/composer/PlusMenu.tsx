import { useQueries } from "@tanstack/react-query";
import {
  Box,
  Compass,
  MonitorSmartphone,
  PanelTop,
  FileText,
  FolderPlus,
  Globe,
  MessageSquareText,
  Paperclip,
  Plug,
  Plus,
  Settings2,
  Shapes,
  Sparkles,
  Wand2,
} from "lucide-react";
import { useState } from "react";
import {
  commands,
  type Attachment,
  type AttachmentLimits,
  type McpPrompt,
  type McpResource,
  type McpServerLists,
  type McpServerState,
} from "@/bindings";
import { IconButton } from "@/components/ui/IconButton";
import {
  MenuCheckboxItem,
  MenuContent,
  MenuItem,
  MenuLabel,
  MenuRoot,
  MenuSeparator,
  MenuSub,
  MenuSubContent,
  MenuSubTrigger,
  MenuTrigger,
} from "@/components/ui/Menu";
import { Tooltip } from "@/components/ui/Tooltip";
import { formatBytes } from "@/features/attachments/limits";
import { useToolSwitches } from "@/features/mcp/chatTools";
import { PromptArgsDialog } from "@/features/mcp/PromptArgsDialog";
import { mcpKeys, useMcpServers } from "@/features/mcp/queries";
import { StatusDot } from "@/features/mcp/ui";
import { DEFAULT_SETTINGS, useSettings } from "@/features/settings/queries";
import { useOpenSettings } from "@/features/settings/useOpenSettings";
import { unwrap } from "@/lib/api";

/**
 * Composer "+" menu: attach files; share a folder; switch connectors and single tools on or off for this
 * chat; attach an MCP prompt or resource as context; web search (and web fetch) on or off.
 */
export function PlusMenu({
  onPickFiles,
  onPickFolder,
  limits,
  conversationId,
  onAttach,
  modelSeesImages = true,
}: {
  onPickFiles: () => void;
  /** Shares a folder with the chat (its file tools work there). */
  onPickFolder: () => void;
  limits: AttachmentLimits;
  /** The chat being written in; `undefined` for a new chat (switches are kept as a draft). */
  conversationId?: string;
  /** Adds attachments the backend prepares (prompts, resources). */
  onAttach: (name: string, run: () => Promise<Attachment[]>) => void;
  /** The chat's model sees images (computer use needs that). */
  modelSeesImages?: boolean;
}) {
  const openSettings = useOpenSettings();
  const [promptToFill, setPromptToFill] = useState<McpPrompt | null>(null);
  const { data: servers } = useMcpServers();
  const switches = useToolSwitches(conversationId);
  const settings = useSettings().data ?? DEFAULT_SETTINGS;
  const sandboxEnabled = settings.sandbox.enabled;
  const browserEnabled = settings.browser.enabled;
  const g = settings.generation;
  const generationReady = (["image", "video", "audio"] as const).some(
    (k) => g[k].model !== "" && (g[k].providerId !== null || g[k].baseUrl !== ""),
  );
  const chromeEnabled = settings.chrome.enabled;
  const computerEnabled = settings.computer.enabled;
  const running = servers.filter((s) => s.status === "running");
  const lists = useQueries({
    queries: running.map((s) => ({
      queryKey: mcpKeys.lists(s.name),
      queryFn: () => commands.mcpServerLists(s.name),
    })),
  });
  const listOf = (name: string): McpServerLists | undefined =>
    lists[running.findIndex((s) => s.name === name)]?.data;
  const prompts = running.flatMap((s) => listOf(s.name)?.prompts ?? []);
  const resources = running.flatMap((s) => listOf(s.name)?.resources ?? []);

  const attachPrompt = (p: McpPrompt, args: Record<string, string>) =>
    onAttach(p.title ?? p.name, async () => [
      await unwrap(commands.mcpAttachPrompt(p.server, p.name, args)),
    ]);
  const attachResource = (r: McpResource) =>
    onAttach(r.title ?? r.name, () => unwrap(commands.mcpAttachResource(r.server, r.uri)));

  return (
    <>
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
            <span className="flex min-w-0 flex-col gap-0.5">
              <span>Add files or photos</span>
              <span className="text-xs leading-4 text-fg-subtle">
                Up to {limits.maxFiles} files · images {formatBytes(limits.maxImageBytes)} · PDFs{" "}
                {formatBytes(limits.maxPdfBytes)} / {limits.maxPdfPages} pages · text{" "}
                {formatBytes(limits.maxTextBytes)}
              </span>
            </span>
          </MenuItem>
          <MenuItem onSelect={onPickFolder}>
            <FolderPlus />
            <span className="flex min-w-0 flex-col gap-0.5">
              <span>Add folder</span>
              <span className="text-xs leading-4 text-fg-subtle">
                Lets Oxide read and, with your approval, change files there
              </span>
            </span>
          </MenuItem>
          {prompts.length > 0 && (
            <MenuSub>
              <MenuSubTrigger>
                <MessageSquareText />
                Add a prompt
              </MenuSubTrigger>
              <MenuSubContent className="max-h-96 w-72 overflow-y-auto">
                {prompts.map((p) => (
                  <MenuItem
                    key={`${p.server}/${p.name}`}
                    onSelect={() =>
                      p.arguments.length > 0 ? setPromptToFill(p) : attachPrompt(p, {})
                    }
                  >
                    <span className="flex min-w-0 flex-col">
                      <span className="truncate">{p.title ?? p.name}</span>
                      <span className="truncate text-xs text-fg-subtle">
                        {p.server}
                        {p.description ? ` · ${p.description}` : ""}
                      </span>
                    </span>
                  </MenuItem>
                ))}
              </MenuSubContent>
            </MenuSub>
          )}
          {resources.length > 0 && (
            <MenuSub>
              <MenuSubTrigger>
                <FileText />
                Add from a connector
              </MenuSubTrigger>
              <MenuSubContent className="max-h-96 w-72 overflow-y-auto">
                {resources.map((r) => (
                  <MenuItem key={`${r.server}/${r.uri}`} onSelect={() => attachResource(r)}>
                    <span className="flex min-w-0 flex-col">
                      <span className="truncate">{r.title ?? r.name}</span>
                      <span className="truncate text-xs text-fg-subtle">
                        {r.server} · {r.uri}
                      </span>
                    </span>
                  </MenuItem>
                ))}
              </MenuSubContent>
            </MenuSub>
          )}
          <MenuSeparator />
          <MenuCheckboxItem
            checked={switches.webOn}
            onCheckedChange={(v) => switches.toggleWeb(v === true)}
            onSelect={(e) => e.preventDefault()}
          >
            <Globe />
            Web search
          </MenuCheckboxItem>
          <MenuCheckboxItem
            checked={switches.artifactsOn}
            onCheckedChange={(v) => switches.toggleArtifacts(v === true)}
            onSelect={(e) => e.preventDefault()}
          >
            <Shapes />
            Artifacts
          </MenuCheckboxItem>
          <MenuCheckboxItem
            checked={switches.saveSkillsOn}
            onCheckedChange={(v) => switches.toggleSaveSkills(v === true)}
            onSelect={(e) => e.preventDefault()}
          >
            <Sparkles />
            <span className="flex min-w-0 flex-col gap-0.5">
              <span>Save skills</span>
              <span className="text-xs leading-4 text-fg-subtle">
                Models may create and change skills; each save asks you first
              </span>
            </span>
          </MenuCheckboxItem>
          {generationReady && (
            <MenuCheckboxItem
              checked={switches.generationOn}
              onCheckedChange={(v) => switches.toggleGeneration(v === true)}
              onSelect={(e) => e.preventDefault()}
            >
              <Wand2 />
              <span className="flex min-w-0 flex-col gap-0.5">
                <span>Generate media</span>
                <span className="text-xs leading-4 text-fg-subtle">
                  Images, video and audio from Settings → Generation; each asks you first
                </span>
              </span>
            </MenuCheckboxItem>
          )}
          {sandboxEnabled && (
            <MenuCheckboxItem
              checked={switches.sandboxOn}
              onCheckedChange={(v) => switches.toggleSandbox(v === true)}
              onSelect={(e) => e.preventDefault()}
            >
              <Box />
              Sandbox
            </MenuCheckboxItem>
          )}
          {browserEnabled && (
            <MenuCheckboxItem
              checked={switches.browserOn}
              onCheckedChange={(v) => switches.toggleBrowser(v === true)}
              onSelect={(e) => e.preventDefault()}
            >
              <Compass />
              Browser
            </MenuCheckboxItem>
          )}
          {chromeEnabled && (
            <MenuCheckboxItem
              checked={switches.chromeOn}
              onCheckedChange={(v) => switches.toggleChrome(v === true)}
              onSelect={(e) => e.preventDefault()}
            >
              <PanelTop />
              Chrome
            </MenuCheckboxItem>
          )}
          {computerEnabled && (
            <MenuCheckboxItem
              checked={switches.computerOn}
              onCheckedChange={(v) => switches.toggleComputer(v === true)}
              onSelect={(e) => e.preventDefault()}
            >
              <MonitorSmartphone />
              <span className="flex min-w-0 flex-col">
                Use the computer
                {!modelSeesImages && (
                  <span className="text-xs text-fg-subtle">
                    This model can&apos;t see images. Mark it in Settings → Models if it can.
                  </span>
                )}
              </span>
            </MenuCheckboxItem>
          )}
          <MenuSub>
            <MenuSubTrigger>
              <Plug />
              Connectors
            </MenuSubTrigger>
            <MenuSubContent className="w-64">
              {servers.length === 0 && (
                <MenuItem disabled>
                  <span className="text-fg-subtle">No connectors yet</span>
                </MenuItem>
              )}
              {servers.map((s) => (
                <ServerSwitches
                  key={s.name}
                  server={s}
                  lists={listOf(s.name)}
                  on={switches.serverOn(s.name)}
                  toolOn={(t) => switches.toolOn(s.name, t)}
                  onServer={(on) => switches.toggleServer(s.name, on)}
                  onTool={(t, on) => switches.toggleTool(s.name, t, on)}
                />
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
      {promptToFill && (
        <PromptArgsDialog
          prompt={promptToFill}
          onClose={() => setPromptToFill(null)}
          onSubmit={(args) => {
            attachPrompt(promptToFill, args);
            setPromptToFill(null);
          }}
        />
      )}
    </>
  );
}

/** One connector in the "+" menu: its switch for this chat and its tools'. */
function ServerSwitches({
  server,
  lists,
  on,
  toolOn,
  onServer,
  onTool,
}: {
  server: McpServerState;
  lists: McpServerLists | undefined;
  on: boolean;
  toolOn: (tool: string) => boolean;
  onServer: (on: boolean) => void;
  onTool: (tool: string, on: boolean) => void;
}) {
  const tools = lists?.tools ?? [];
  const label = (
    <span className="flex min-w-0 items-center gap-2">
      <StatusDot status={server.status} />
      <span className="truncate">{server.name}</span>
    </span>
  );
  if (server.status !== "running" || tools.length === 0) {
    return (
      <MenuCheckboxItem
        checked={on}
        onCheckedChange={(v) => onServer(v === true)}
        onSelect={(e) => e.preventDefault()}
        aria-label={`Use ${server.name} in this chat`}
      >
        {label}
      </MenuCheckboxItem>
    );
  }
  const enabledCount = tools.filter((t) => toolOn(t.name)).length;
  return (
    <MenuSub>
      <MenuSubTrigger aria-label={`${server.name} tools`}>
        {label}
        <span className="ml-auto pl-2 text-xs text-fg-subtle">
          {on ? `${enabledCount}/${tools.length}` : "off"}
        </span>
      </MenuSubTrigger>
      <MenuSubContent className="max-h-96 w-72 overflow-y-auto">
        <MenuCheckboxItem
          checked={on}
          onCheckedChange={(v) => onServer(v === true)}
          onSelect={(e) => e.preventDefault()}
          aria-label={`Use ${server.name} in this chat`}
        >
          Use in this chat
        </MenuCheckboxItem>
        <MenuSeparator />
        <MenuLabel>Tools</MenuLabel>
        {tools.map((t) => (
          <MenuCheckboxItem
            key={t.name}
            disabled={!on}
            checked={on && toolOn(t.name)}
            onCheckedChange={(v) => onTool(t.name, v === true)}
            onSelect={(e) => e.preventDefault()}
          >
            <span className="truncate font-mono text-sm">{t.name}</span>
          </MenuCheckboxItem>
        ))}
      </MenuSubContent>
    </MenuSub>
  );
}
