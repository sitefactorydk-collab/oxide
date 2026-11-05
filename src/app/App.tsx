import { useEffect, useState } from "react";
import { commands, type PingResponse } from "@/bindings";

type PingState =
  { status: "loading" } | { status: "ok"; data: PingResponse } | { status: "error"; error: string };

export default function App() {
  const [ping, setPing] = useState<PingState>({ status: "loading" });

  useEffect(() => {
    let cancelled = false;
    commands
      .ping("Oxide")
      .then((data) => {
        if (!cancelled) setPing({ status: "ok", data });
      })
      .catch((err: unknown) => {
        if (!cancelled) setPing({ status: "error", error: String(err) });
      });
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <main className="flex h-screen flex-col items-center justify-center gap-4 bg-surface text-fg">
      <img src="/oxide-mark.svg" alt="" className="h-16 w-16" />
      <h1 className="font-serif text-4xl tracking-tight">Oxide</h1>
      <p className="text-sm text-fg-muted" data-testid="ping-status">
        {ping.status === "loading" && "Connecting to backend…"}
        {ping.status === "ok" &&
          `${ping.data.message} · v${ping.data.appVersion} (core ${ping.data.coreVersion})`}
        {ping.status === "error" && `Backend error: ${ping.error}`}
      </p>
    </main>
  );
}
