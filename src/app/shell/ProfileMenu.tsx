import {
  ChevronsUpDown,
  CircleHelp,
  Info,
  Keyboard,
  Monitor,
  Moon,
  Palette,
  Settings,
  Sun,
  UserRound,
} from "lucide-react";
import { shortcutText } from "@/app/shortcuts";
import {
  MenuContent,
  MenuItem,
  MenuLabel,
  MenuRadioGroup,
  MenuRadioItem,
  MenuRoot,
  MenuSeparator,
  MenuSub,
  MenuSubContent,
  MenuSubTrigger,
  MenuTrigger,
} from "@/components/ui/Menu";
import { useOpenSettings } from "@/features/settings/useOpenSettings";
import { displayName, initials } from "@/lib/profile";
import { useUi, type ThemePref } from "@/stores/ui";

export function Avatar({ name, className = "size-7" }: { name: string; className?: string }) {
  return (
    <span
      aria-hidden
      className={`inline-flex shrink-0 items-center justify-center rounded-full bg-fg text-xs font-semibold text-app ${className}`}
    >
      {name.trim() ? initials(name) : <UserRound className="size-4" />}
    </span>
  );
}

/** Bottom-of-sidebar profile button with Settings, Appearance and Help. */
export function ProfileMenu() {
  const userName = useUi((s) => s.userName);
  const theme = useUi((s) => s.theme);
  const setTheme = useUi((s) => s.setTheme);
  const setShortcutsOpen = useUi((s) => s.setShortcutsOpen);
  const openSettings = useOpenSettings();

  return (
    <MenuRoot>
      <MenuTrigger asChild>
        <button
          type="button"
          aria-label="Profile and settings"
          className="flex h-11 w-full items-center gap-2.5 rounded-lg px-2 text-left hover:bg-hover data-[state=open]:bg-hover"
        >
          <Avatar name={userName} />
          <span className="min-w-0 flex-1">
            <span className="block truncate text-md text-fg">{displayName(userName)}</span>
            <span className="block text-xs text-fg-subtle">Local profile</span>
          </span>
          <ChevronsUpDown className="size-4 shrink-0 text-fg-subtle" />
        </button>
      </MenuTrigger>
      <MenuContent
        side="top"
        align="start"
        className="w-[var(--radix-dropdown-menu-trigger-width)]"
      >
        <MenuLabel>{displayName(userName)}</MenuLabel>
        <MenuItem shortcut={shortcutText("settings")} onSelect={() => openSettings("general")}>
          <Settings />
          Settings
        </MenuItem>
        <MenuSub>
          <MenuSubTrigger>
            <Palette />
            Appearance
          </MenuSubTrigger>
          <MenuSubContent>
            <MenuRadioGroup value={theme} onValueChange={(v) => setTheme(v as ThemePref)}>
              <MenuRadioItem value="light">
                <Sun />
                Light
              </MenuRadioItem>
              <MenuRadioItem value="dark">
                <Moon />
                Dark
              </MenuRadioItem>
              <MenuRadioItem value="system">
                <Monitor />
                Match system
              </MenuRadioItem>
            </MenuRadioGroup>
            <MenuSeparator />
            <MenuItem onSelect={() => openSettings("appearance")}>
              More appearance settings…
            </MenuItem>
          </MenuSubContent>
        </MenuSub>
        <MenuSub>
          <MenuSubTrigger>
            <CircleHelp />
            Help
          </MenuSubTrigger>
          <MenuSubContent>
            <MenuItem shortcut={shortcutText("shortcuts")} onSelect={() => setShortcutsOpen(true)}>
              <Keyboard />
              Keyboard shortcuts
            </MenuItem>
            <MenuItem onSelect={() => openSettings("about")}>
              <Info />
              About Oxide
            </MenuItem>
          </MenuSubContent>
        </MenuSub>
      </MenuContent>
    </MenuRoot>
  );
}
