import type { QueryClient } from "@tanstack/react-query";
import type { ChatTools } from "@/bindings";
import { mcpKeys, useChatTools, useSetChatTools } from "./queries";
import { EMPTY_TOOLS, useMcpUi } from "./store";

/**
 * The tool switches of the chat being written in: a saved chat's own, or (before the chat
 * exists) a draft that `applyDraftChatTools` saves once it's created.
 */
export function useToolSwitches(conversationId: string | undefined) {
  const saved = useChatTools(conversationId);
  const setSaved = useSetChatTools();
  const draft = useMcpUi((s) => s.draftTools);
  const setDraft = useMcpUi((s) => s.setDraftTools);
  const tools = conversationId ? saved.data : draft;

  const set = (next: Required<ChatTools>) => {
    if (conversationId) setSaved.mutate({ id: conversationId, tools: next });
    else setDraft(next);
  };

  const toggleServer = (server: string, on: boolean) =>
    set({
      ...tools,
      disabledServers: on
        ? tools.disabledServers.filter((s) => s !== server)
        : [...new Set([...tools.disabledServers, server])],
    });

  const toggleTool = (server: string, tool: string, on: boolean) => {
    const key = `${server}/${tool}`;
    set({
      ...tools,
      disabledTools: on
        ? tools.disabledTools.filter((t) => t !== key)
        : [...new Set([...tools.disabledTools, key])],
    });
  };

  /** Web search and web fetch are switched together ("Web search" in the "+" menu). */
  const webOn = !tools.disabledBuiltins.includes("web_search");
  const toggleWeb = (on: boolean) =>
    set({
      ...tools,
      disabledBuiltins: on
        ? tools.disabledBuiltins.filter((b) => b !== "web_search" && b !== "web_fetch")
        : [...new Set([...tools.disabledBuiltins, "web_search", "web_fetch"])],
    });

  /** The `artifact` tool ("Artifacts" in the "+" menu). */
  const artifactsOn = !tools.disabledBuiltins.includes("artifact");
  const toggleArtifacts = (on: boolean) =>
    set({
      ...tools,
      disabledBuiltins: on
        ? tools.disabledBuiltins.filter((b) => b !== "artifact")
        : [...new Set([...tools.disabledBuiltins, "artifact"])],
    });

  /** The Docker sandbox ("Sandbox" in the "+" menu, when it's on in Settings). */
  const sandboxOn = !tools.disabledBuiltins.includes("sandbox");
  const toggleSandbox = (on: boolean) =>
    set({
      ...tools,
      disabledBuiltins: on
        ? tools.disabledBuiltins.filter((b) => b !== "sandbox")
        : [...new Set([...tools.disabledBuiltins, "sandbox"])],
    });

  /** The browser ("Browser" in the "+" menu, when it's on in Settings). */
  const browserOn = !tools.disabledBuiltins.includes("browser");
  const toggleBrowser = (on: boolean) =>
    set({
      ...tools,
      disabledBuiltins: on
        ? tools.disabledBuiltins.filter((b) => b !== "browser")
        : [...new Set([...tools.disabledBuiltins, "browser"])],
    });

  /** The user's Chrome ("Chrome" in the "+" menu, when it's on in Settings). */
  const chromeOn = !tools.disabledBuiltins.includes("chrome");
  const toggleChrome = (on: boolean) =>
    set({
      ...tools,
      disabledBuiltins: on
        ? tools.disabledBuiltins.filter((b) => b !== "chrome")
        : [...new Set([...tools.disabledBuiltins, "chrome"])],
    });

  /** Computer use ("Use the computer" in the "+" menu, when it's on in Settings). */
  const computerOn = !tools.disabledBuiltins.includes("computer");
  const toggleComputer = (on: boolean) =>
    set({
      ...tools,
      disabledBuiltins: on
        ? tools.disabledBuiltins.filter((b) => b !== "computer")
        : [...new Set([...tools.disabledBuiltins, "computer"])],
    });

  /** `save_skill` ("Save skills" in the "+" menu): models may create and change skills. */
  const saveSkillsOn = !tools.disabledBuiltins.includes("save_skill");
  const toggleSaveSkills = (on: boolean) =>
    set({
      ...tools,
      disabledBuiltins: on
        ? tools.disabledBuiltins.filter((b) => b !== "save_skill")
        : [...new Set([...tools.disabledBuiltins, "save_skill"])],
    });

  return {
    tools,
    saveSkillsOn,
    toggleSaveSkills,
    chromeOn,
    toggleChrome,
    computerOn,
    toggleComputer,
    sandboxOn,
    toggleSandbox,
    browserOn,
    toggleBrowser,
    webOn,
    toggleWeb,
    artifactsOn,
    toggleArtifacts,
    serverOn: (server: string) => !tools.disabledServers.includes(server),
    toolOn: (server: string, tool: string) => !tools.disabledTools.includes(`${server}/${tool}`),
    toggleServer,
    toggleTool,
  };
}

/** The draft switches of a chat not created yet, or `null` when everything is on. */
export function draftChatTools(): Required<ChatTools> | null {
  const draft = useMcpUi.getState().draftTools;
  const empty =
    draft.disabledServers.length === 0 &&
    draft.disabledTools.length === 0 &&
    draft.disabledBuiltins.length === 0;
  return empty ? null : draft;
}

/**
 * A chat was just created with the draft switches (`startConversation` saved them before its
 * first reply): shows them as the chat's own and clears the draft.
 */
export function applyDraftChatTools(qc: QueryClient, conversationId: string): void {
  const draft = draftChatTools();
  if (draft === null) return;
  qc.setQueryData(mcpKeys.chatTools(conversationId), draft);
  useMcpUi.getState().setDraftTools(EMPTY_TOOLS);
}
