import { useEffect, useState } from "react";
import { setWindowTheme } from "@/lib/window";
import { useUi, type ThemePref } from "@/stores/ui";

const DARK_QUERY = "(prefers-color-scheme: dark)";

function systemPrefersDark(): boolean {
  return typeof window.matchMedia === "function" && window.matchMedia(DARK_QUERY).matches;
}

export function resolveTheme(pref: ThemePref, systemDark: boolean): "light" | "dark" {
  if (pref === "system") return systemDark ? "dark" : "light";
  return pref;
}

function useSystemDark(): boolean {
  const [systemDark, setSystemDark] = useState(systemPrefersDark);
  useEffect(() => {
    if (typeof window.matchMedia !== "function") return;
    const mq = window.matchMedia(DARK_QUERY);
    const onChange = () => setSystemDark(mq.matches);
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, []);
  return systemDark;
}

/** The theme in effect: the preference, or the OS's when it's "system". */
export function useResolvedTheme(): "light" | "dark" {
  return resolveTheme(
    useUi((s) => s.theme),
    useSystemDark(),
  );
}

/** Applies theme + chat font to <html> and follows the OS when the preference is "system". */
export function useThemeSync(): void {
  const theme = useUi((s) => s.theme);
  const chatFont = useUi((s) => s.chatFont);
  const systemDark = useSystemDark();

  useEffect(() => {
    const root = document.documentElement;
    root.dataset.theme = resolveTheme(theme, systemDark);
    root.dataset.chatFont = chatFont;
  }, [theme, systemDark, chatFont]);

  useEffect(() => {
    // Native chrome (macOS traffic lights, Linux decorations) follows the app theme.
    setWindowTheme(theme === "system" ? null : theme).catch(() => {});
  }, [theme]);
}
