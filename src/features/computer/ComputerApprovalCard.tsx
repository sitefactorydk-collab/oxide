import { MonitorSmartphone } from "lucide-react";
import { useState } from "react";
import type { ApprovalDecision, ComputerApproval, ToolApprovalRequest } from "@/bindings";
import { useStreams } from "@/features/chat/streams";
import { useAnswerApproval } from "@/features/tools/queries";
import { DEFAULT_SETTINGS, useSetComputerSettings, useSettings } from "@/features/settings/queries";
import { errorMessage } from "@/lib/api";

/**
 * Computer use's approval cards: the first asks whether a chat may control the computer at
 * all (for the whole chat, or not); later ones ask for each app the chat reaches.
 */
export function ComputerApprovalCard({
  request,
  computer,
}: {
  request: ToolApprovalRequest;
  computer: ComputerApproval;
}) {
  const answer = useAnswerApproval();
  const settings = useSettings().data ?? DEFAULT_SETTINGS;
  const setComputer = useSetComputerSettings();
  const [always, setAlways] = useState(false);
  const app = computer.kind === "app" ? computer.app : null;
  const send = (decision: ApprovalDecision) => {
    useStreams
      .getState()
      .markAnswered(request.conversationId, request.toolUseId, decision !== "deny");
    if (app && always && decision !== "deny") {
      const apps = settings.computer.alwaysAllowedApps;
      if (!apps.some((a) => a.id === app.id)) {
        setComputer({ alwaysAllowedApps: [...apps, app] });
      }
    }
    answer.mutate({
      conversationId: request.conversationId,
      toolUseId: request.toolUseId,
      decision,
    });
  };
  return (
    <div
      role="group"
      aria-label={app ? `Allow Oxide to use ${app.name}` : "Allow Oxide to control your computer"}
      className="flex flex-col gap-3 border-t border-line px-3 pb-3 pt-3"
    >
      <div className="flex items-start gap-2.5">
        <MonitorSmartphone aria-hidden className="mt-0.5 size-4 shrink-0 text-accent" />
        <div className="min-w-0 text-md">
          {app ? (
            <p className="text-fg">
              Allow Oxide to use <strong className="font-semibold">{app.name}</strong>?
            </p>
          ) : (
            <p className="text-fg">Let Oxide control your computer in this chat?</p>
          )}
          <p className="mt-0.5 text-sm text-fg-subtle">
            Next: {computer.action}
            {computer.display && !app ? ` · on ${computer.display}` : ""}
          </p>
        </div>
      </div>
      {app ? (
        <p className="text-sm text-fg-muted">
          The model wants to click, type or press keys in {app.name}. It asks again for every other
          app.
        </p>
      ) : (
        <ul className="flex list-disc flex-col gap-1 pl-5 text-sm text-fg-muted">
          <li>
            The model sees your screen and uses the mouse and keyboard, like a person at your
            computer. Avoid using the computer yourself meanwhile.
          </li>
          <li>Oxide asks before it uses each app, and never uses itself or a password manager.</li>
          <li>
            It never types into password or payment fields, or types card or bank numbers: you enter
            those.
          </li>
          <li>
            A bar at the top of the screen shows what it does. Stop it there or with{" "}
            <kbd className="rounded border border-line px-1 font-mono text-xs">
              {computer.panicShortcut}
            </kbd>
            .
          </li>
        </ul>
      )}
      {app && (
        <label className="flex items-center gap-2 text-sm text-fg-muted">
          <input
            type="checkbox"
            checked={always}
            onChange={(e) => setAlways(e.target.checked)}
            className="size-4 accent-[var(--ox-accent)]"
          />
          Always allow {app.name} in every chat
        </label>
      )}
      <div className="flex flex-wrap justify-end gap-2">
        <button
          type="button"
          disabled={answer.isPending}
          onClick={() => send("deny")}
          className="h-8 rounded-lg px-3 text-sm text-fg-muted hover:bg-hover hover:text-fg disabled:opacity-50"
        >
          Deny
        </button>
        {app && (
          <button
            type="button"
            disabled={answer.isPending}
            onClick={() => send("allowOnce")}
            className="h-8 rounded-lg border border-line-strong px-3 text-sm text-fg hover:bg-hover disabled:opacity-50"
          >
            Allow once
          </button>
        )}
        <button
          type="button"
          autoFocus
          disabled={answer.isPending}
          onClick={() => send("allowChat")}
          className="h-8 rounded-lg bg-fg px-3 text-sm font-medium text-app disabled:opacity-50"
        >
          Allow for this chat
        </button>
      </div>
      {answer.error && (
        <p role="alert" className="text-sm text-danger">
          {errorMessage(answer.error)}
        </p>
      )}
    </div>
  );
}
