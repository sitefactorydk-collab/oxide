import type { ComponentProps } from "react";
import { RouterProvider } from "react-router";
import { AppProviders } from "./AppProviders";

type AppRouter = ComponentProps<typeof RouterProvider>["router"];

/** App root: global providers + the hash data router created in `main.tsx`. */
export default function App({ router }: { router: AppRouter }) {
  return (
    <AppProviders>
      <RouterProvider router={router} />
    </AppProviders>
  );
}
