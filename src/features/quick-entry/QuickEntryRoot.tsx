import { QueryClientProvider } from "@tanstack/react-query";
import { Tooltip } from "radix-ui";
import { useState } from "react";
import { useThemeSync } from "@/app/useThemeSync";
import { createQueryClient } from "@/lib/api";
import { QuickEntry } from "./QuickEntry";

/**
 * Root of the quick entry window (`index.html?window=quick`). Lighter than the app's
 * providers: no router, and no settings sync, so this window never writes the UI cache the
 * main window owns (it re-reads it whenever it's shown).
 */
export function QuickEntryRoot() {
  const [client] = useState(createQueryClient);
  return (
    <QueryClientProvider client={client}>
      <Shell />
    </QueryClientProvider>
  );
}

function Shell() {
  useThemeSync();
  return (
    <Tooltip.Provider delayDuration={500} skipDelayDuration={150}>
      <QuickEntry />
    </Tooltip.Provider>
  );
}
