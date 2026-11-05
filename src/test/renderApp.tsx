import { render } from "@testing-library/react";
import { MemoryRouter, Route, Routes, useLocation } from "react-router";
import { AppLayout } from "@/app/AppLayout";
import { AppProviders } from "@/app/AppProviders";
import { createQueryClient } from "@/lib/api";
import { defaultUiData, useUi } from "@/stores/ui";
import { resetFakeBackend, type FakeSeed } from "./fakeBackend";

/**
 * Renders the whole app at `path` with fresh UI state, a fresh query cache and a freshly
 * seeded fake backend (`backend` tweaks the seed).
 *
 * Uses the declarative MemoryRouter rather than a data router: data-router navigations build
 * a Node `Request` with jsdom's AbortSignal, which Node rejects ("Expected signal to be an
 * instance of AbortSignal"). The app itself uses the hash data router (see main.tsx).
 */
export function renderApp(
  path = "/new",
  { ui = {}, backend = {} }: { ui?: Partial<typeof defaultUiData>; backend?: FakeSeed } = {},
) {
  // Tests start from stored settings, not from a Phase 1 cache.
  useUi.setState({ ...defaultUiData, settingsMigrated: true, ...ui });
  resetFakeBackend(backend);
  const client = createQueryClient();
  const location = { pathname: path };
  function LocationProbe() {
    location.pathname = useLocation().pathname;
    return null;
  }
  const result = render(
    <AppProviders client={client}>
      <MemoryRouter initialEntries={[path]}>
        <LocationProbe />
        <Routes>
          <Route path="*" element={<AppLayout />} />
        </Routes>
      </MemoryRouter>
    </AppProviders>,
  );
  return { ...result, location, client };
}
