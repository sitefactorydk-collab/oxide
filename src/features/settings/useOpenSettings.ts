import { useCallback } from "react";
import { useLocation, useNavigate } from "react-router";

export interface SettingsLocationState {
  /** Page shown behind the settings modal and restored on close. */
  from?: string;
}

export function isSettingsPath(pathname: string): boolean {
  return pathname === "/settings" || pathname.startsWith("/settings/");
}

/** Opens `/settings/<section>` over the current page, remembering where to return. */
export function useOpenSettings(): (section?: string) => void {
  const navigate = useNavigate();
  const location = useLocation();
  return useCallback(
    (section = "general") => {
      const state = location.state as SettingsLocationState | null;
      const from = isSettingsPath(location.pathname) ? (state?.from ?? "/new") : location.pathname;
      navigate(`/settings/${section}`, {
        state: { from } satisfies SettingsLocationState,
        replace: isSettingsPath(location.pathname),
      });
    },
    [navigate, location.pathname, location.state],
  );
}
