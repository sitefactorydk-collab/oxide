import { Tooltip } from "radix-ui";
import type { ReactNode } from "react";
import { useThemeSync } from "./useThemeSync";

/** Global providers shared by the app and tests (theme sync, tooltip timing). */
export function AppProviders({ children }: { children: ReactNode }) {
  useThemeSync();
  return (
    <Tooltip.Provider delayDuration={500} skipDelayDuration={150}>
      {children}
    </Tooltip.Provider>
  );
}
