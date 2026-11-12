// Runs in the page's own world (injected when Oxide starts working in a tab) and passes
// console messages and uncaught errors to Oxide's content script as `__oxide_console` events.
// Only messages from then on are seen. The page's console keeps working as before.

(() => {
  const w = window as unknown as { __oxideConsoleHooked?: boolean };
  if (w.__oxideConsoleHooked) return;
  w.__oxideConsoleHooked = true;

  const show = (v: unknown): string => {
    if (typeof v === "string") return v;
    if (v instanceof Error) return `${v.name}: ${v.message}`;
    try {
      return JSON.stringify(v) ?? String(v);
    } catch {
      return String(v);
    }
  };
  // Messages wait here until Oxide's content script says hello (it may arrive later).
  const buffer: string[] = [];
  let streaming = false;
  const dispatch = (detail: string) => {
    try {
      window.dispatchEvent(new CustomEvent("__oxide_console", { detail }));
    } catch {
      // Never break the page.
    }
  };
  const send = (level: string, text: string) => {
    const detail = JSON.stringify({ level, text: text.slice(0, 4000), time: Date.now() });
    if (streaming) dispatch(detail);
    else if (buffer.length < 500) buffer.push(detail);
  };
  window.addEventListener("__oxide_console_hello", () => {
    streaming = true;
    for (const d of buffer.splice(0)) dispatch(d);
  });
  const c = console as unknown as Record<string, (...args: unknown[]) => void>;
  for (const level of ["log", "info", "warn", "error", "debug"]) {
    const original = c[level];
    if (typeof original !== "function") continue;
    c[level] = function (this: unknown, ...args: unknown[]) {
      send(level, args.map(show).join(" "));
      return original.apply(this, args);
    };
  }
  window.addEventListener("error", (e) => {
    send("exception", `${e.message} (${e.filename}:${e.lineno}:${e.colno})`);
  });
  window.addEventListener("unhandledrejection", (e) => {
    send("exception", `Unhandled promise rejection: ${show(e.reason)}`);
  });
})();
