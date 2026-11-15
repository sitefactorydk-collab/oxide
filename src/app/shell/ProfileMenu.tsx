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
import { useSetTheme } from "@/features/settings/queries";
import { useOpenSettings } from "@/features/settings/useOpenSettings";
import { useT } from "@/i18n";
import { initials } from "@/lib/profile";
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
  const setTheme = useSetTheme();
  const setShortcutsOpen = useUi((s) => s.setShortcutsOpen);
  const openSettings = useOpenSettings();
  const t = useT();
  const shown = userName.trim() || t("profile.you");

  return (
    <MenuRoot>
      <MenuTrigger asChild>
        <button
          type="button"
          aria-label={t("profile.menu")}
          className="flex h-11 w-full items-center gap-2.5 rounded-lg px-2 text-left hover:bg-hover data-[state=open]:bg-hover"
        >
          <Avatar name={userName} />
          <span className="min-w-0 flex-1">
            <span className="block truncate text-md text-fg">{shown}</span>
            <span className="block text-xs text-fg-subtle">{t("profile.local")}</span>
          </span>
          <ChevronsUpDown className="size-4 shrink-0 text-fg-subtle" />
        </button>
      </MenuTrigger>
      <MenuContent
        side="top"
        align="start"
        className="w-[var(--radix-dropdown-menu-trigger-width)]"
      >
        <MenuLabel>{shown}</MenuLabel>
        <MenuItem shortcut={shortcutText("settings")} onSelect={() => openSettings("general")}>
          <Settings />
          {t("menu.settings")}
        </MenuItem>
        <MenuSub>
          <MenuSubTrigger>
            <Palette />
            {t("menu.appearance")}
          </MenuSubTrigger>
          <MenuSubContent>
            <MenuRadioGroup value={theme} onValueChange={(v) => setTheme(v as ThemePref)}>
              <MenuRadioItem value="light">
                <Sun />
                {t("menu.light")}
              </MenuRadioItem>
              <MenuRadioItem value="dark">
                <Moon />
                {t("menu.dark")}
              </MenuRadioItem>
              <MenuRadioItem value="system">
                <Monitor />
                {t("menu.system")}
              </MenuRadioItem>
            </MenuRadioGroup>
            <MenuSeparator />
            <MenuItem onSelect={() => openSettings("appearance")}>
              {t("menu.moreAppearance")}
            </MenuItem>
          </MenuSubContent>
        </MenuSub>
        <MenuSub>
          <MenuSubTrigger>
            <CircleHelp />
            {t("menu.help")}
          </MenuSubTrigger>
          <MenuSubContent>
            <MenuItem shortcut={shortcutText("shortcuts")} onSelect={() => setShortcutsOpen(true)}>
              <Keyboard />
              {t("menu.shortcuts")}
            </MenuItem>
            <MenuItem onSelect={() => openSettings("about")}>
              <Info />
              {t("menu.about")}
            </MenuItem>
          </MenuSubContent>
        </MenuSub>
      </MenuContent>
    </MenuRoot>
  );
}
