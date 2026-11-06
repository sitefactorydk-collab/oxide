import { matchPath, useLocation, useNavigate } from "react-router";
import { Toaster } from "@/components/ui/Toaster";
import { useChatEvents } from "@/features/chat/useChatEvents";
import { McpAskDialog } from "@/features/mcp/McpAskDialog";
import { useMcpEvents } from "@/features/mcp/useMcpEvents";
import { SearchDialog } from "@/features/search/SearchDialog";
import { SettingsDialog } from "@/features/settings/SettingsDialog";
import {
  isSettingsPath,
  useOpenSettings,
  type SettingsLocationState,
} from "@/features/settings/useOpenSettings";
import { ShortcutsDialog } from "@/features/shortcuts/ShortcutsDialog";
import { useUi } from "@/stores/ui";
import { PageRoutes } from "./PageRoutes";
import { Sidebar } from "./shell/Sidebar";
import { TitleBar } from "./shell/TitleBar";
import { useGlobalShortcuts } from "./useGlobalShortcuts";

/**
 * Root layout: title bar, sidebar, the current page and app-wide overlays.
 * The data router has a single splat route pointing here; pages are matched by `PageRoutes`
 * so the settings modal can keep the previous page visible behind it.
 */
export function AppLayout() {
  const location = useLocation();
  const navigate = useNavigate();
  const openSettings = useOpenSettings();
  const sidebarOpen = useUi((s) => s.sidebarOpen);
  useGlobalShortcuts();
  useChatEvents();
  useMcpEvents();

  const settingsOpen = isSettingsPath(location.pathname);
  const settingsSection = settingsOpen ? location.pathname.split("/")[2] : undefined;
  const from = (location.state as SettingsLocationState | null)?.from ?? "/new";
  const pagePath = settingsOpen ? from : location.pathname;
  const activeChatId = matchPath("/chat/:id", pagePath)?.params.id;

  return (
    <div className="flex h-full flex-col bg-app text-fg">
      <TitleBar pagePath={pagePath} />
      <div className="flex min-h-0 flex-1">
        {sidebarOpen && <Sidebar activeChatId={activeChatId} />}
        <main className="relative flex min-w-0 flex-1 flex-col">
          <PageRoutes location={settingsOpen ? from : location} />
        </main>
      </div>

      <SettingsDialog
        open={settingsOpen}
        sectionId={settingsSection}
        onSelect={(id) => openSettings(id)}
        onClose={() => navigate(from, { replace: true })}
      />
      <SearchDialog />
      <ShortcutsDialog />
      <McpAskDialog />
      <Toaster />
    </div>
  );
}
