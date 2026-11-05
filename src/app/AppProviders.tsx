import { QueryClientProvider, type QueryClient } from "@tanstack/react-query";
import { Tooltip } from "radix-ui";
import { useState, type ReactNode } from "react";
import { useSettingsSync } from "@/features/settings/queries";
import { createQueryClient } from "@/lib/api";
import { useThemeSync } from "./useThemeSync";

/** Global providers shared by the app and tests: backend data, settings, theme, tooltips. */
export function AppProviders({ children, client }: { children: ReactNode; client?: QueryClient }) {
  const [queryClient] = useState(() => client ?? createQueryClient());
  return (
    <QueryClientProvider client={queryClient}>
      <Shell>{children}</Shell>
    </QueryClientProvider>
  );
}

function Shell({ children }: { children: ReactNode }) {
  useSettingsSync();
  useThemeSync();
  return (
    <Tooltip.Provider delayDuration={500} skipDelayDuration={150}>
      {children}
    </Tooltip.Provider>
  );
}
