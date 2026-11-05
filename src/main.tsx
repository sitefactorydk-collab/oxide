import "@fontsource-variable/inter";
import "@fontsource-variable/source-serif-4";
import "@fontsource-variable/jetbrains-mono";
import "katex/dist/katex.min.css";
import "@/styles/index.css";

import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { createHashRouter } from "react-router";
import App from "@/app/App";
import { AppLayout } from "@/app/AppLayout";

const root = document.getElementById("root");
if (!root) throw new Error("#root element missing from index.html");

// Hash history: works from Tauri's asset protocol without server-side fallbacks.
const router = createHashRouter([{ path: "*", element: <AppLayout /> }]);

createRoot(root).render(
  <StrictMode>
    <App router={router} />
  </StrictMode>,
);
