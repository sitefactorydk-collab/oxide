import { render } from "@testing-library/react";
import { MemoryRouter, Route, Routes, useLocation } from "react-router";
import { AppLayout } from "@/app/AppLayout";
import { AppProviders } from "@/app/AppProviders";
import { useMock } from "@/features/mock/store";
import { defaultUiData, useUi } from "@/stores/ui";

/**
 * Renders the whole app at `path` with fresh UI + mock state.
 *
 * Uses the declarative MemoryRouter rather than a data router: data-router navigations build
 * a Node `Request` with jsdom's AbortSignal, which Node rejects ("Expected signal to be an
 * instance of AbortSignal"). The app itself uses the hash data router (see main.tsx).
 */
export function renderApp(path = "/new", ui: Partial<typeof defaultUiData> = {}) {
  useUi.setState({ ...defaultUiData, ...ui });
  useMock.getState().reset();
  const location = { pathname: path };
  function LocationProbe() {
    location.pathname = useLocation().pathname;
    return null;
  }
  const result = render(
    <AppProviders>
      <MemoryRouter initialEntries={[path]}>
        <LocationProbe />
        <Routes>
          <Route path="*" element={<AppLayout />} />
        </Routes>
      </MemoryRouter>
    </AppProviders>,
  );
  return { ...result, location };
}
