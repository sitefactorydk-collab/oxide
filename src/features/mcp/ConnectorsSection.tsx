import {
  ChevronDown,
  ChevronRight,
  Download,
  FileJson,
  LogIn,
  LogOut,
  Plus,
  RotateCw,
} from "lucide-react";
import { useId, useState } from "react";
import type { McpServerConfig, McpServerState, McpTransport } from "@/bindings";
import { useBrowserStatus } from "@/features/browser/queries";
import { useDockerStatus } from "@/features/sandbox/queries";
import { DEFAULT_SETTINGS, useSetBrowserSettings, useSettings } from "@/features/settings/queries";
import { useOpenSettings } from "@/features/settings/useOpenSettings";
import { errorMessage } from "@/lib/api";
import { cn } from "@/lib/cn";
import { toast } from "@/lib/toast";
import { ImportDialog } from "./ImportDialog";
import {
  useMcpConfig,
  useMcpServerConfig,
  useMcpServers,
  useMcpSignIn,
  useMcpSignOut,
  useRemoveMcpServer,
  useRestartMcpServer,
  useSaveMcpServer,
  useSetMcpEnabled,
} from "./queries";
import { formatPairs, parseArgs, parsePairs } from "./schema";
import { ServerDetails } from "./ServerDetails";
import { countsLabel, protocolLabel, STATUS_LABEL } from "./status";
import { Button, FormRow, inputClass, StatusDot, Switch, textareaClass } from "./ui";

/** Settings → Connectors: MCP servers, their status, tools, logs, and adding servers. */
export function ConnectorsSection() {
  const { data: servers, isPending } = useMcpServers();
  const config = useMcpConfig();
  const openSettings = useOpenSettings();
  const [adding, setAdding] = useState(false);
  const [importing, setImporting] = useState(false);

  return (
    <div className="flex max-w-3xl flex-col gap-3">
      <div className="mb-2 flex flex-wrap gap-2">
        <Button onClick={() => setAdding(true)} disabled={adding}>
          <Plus />
          Add server
        </Button>
        <Button onClick={() => setImporting(true)}>
          <Download />
          Import from Claude desktop
        </Button>
        <Button onClick={() => openSettings("developer")}>
          <FileJson />
          Edit config
        </Button>
      </div>

      {config.data?.error && (
        <p role="alert" className="rounded-lg bg-danger-soft px-3 py-2 text-base text-fg">
          The config file has a problem, so servers keep their last working settings:{" "}
          {config.data.error}
        </p>
      )}

      {adding && (
        <div className="rounded-xl border border-line-strong bg-raised p-4 shadow-sm">
          <ServerForm onDone={() => setAdding(false)} />
        </div>
      )}

      {isPending ? (
        <p className="text-fg-muted">Loading…</p>
      ) : servers.length === 0 && !adding ? (
        <div className="rounded-xl border border-dashed border-line-strong p-6 text-center text-fg-muted">
          <p>No servers yet.</p>
          <p className="mt-1 text-sm text-fg-subtle">
            Add one, or import the servers you set up in Claude desktop.
          </p>
        </div>
      ) : (
        servers.map((s) => <ServerCard key={s.name} server={s} />)
      )}

      <BuiltInBrowserServer />
      <BuiltInSandboxServer />

      <p className="mt-2 text-sm text-fg-subtle">
        Servers are stored in <span className="font-mono">oxide_config.json</span>, which uses the
        same <span className="font-mono">mcpServers</span> format as Claude desktop, so entries can
        be pasted between them. Local servers run on your computer with your permissions; only add
        servers you trust.
      </p>

      {importing && <ImportDialog onClose={() => setImporting(false)} />}
    </div>
  );
}

function ServerCard({ server }: { server: McpServerState }) {
  const openSettings = useOpenSettings();
  const [open, setOpen] = useState(false);
  const [editing, setEditing] = useState(false);
  const setEnabled = useSetMcpEnabled();
  const restart = useRestartMcpServer();
  const signIn = useMcpSignIn();
  const signOut = useMcpSignOut();
  const enabled = server.status !== "disabled";
  const counts = server.status === "running" ? countsLabel(server) : "";
  const proto = protocolLabel(server);

  const run = (p: Promise<unknown>, done?: string) =>
    p.then(() => done && toast(done)).catch((e: unknown) => toast(errorMessage(e), "error"));

  return (
    <section
      aria-label={server.name}
      className={cn(
        "rounded-xl border bg-raised shadow-sm",
        editing ? "border-line-strong" : "border-line",
      )}
    >
      {editing ? (
        <div className="p-4">
          <ServerForm name={server.name} onDone={() => setEditing(false)} />
        </div>
      ) : (
        <>
          <div className="flex items-start gap-3 p-4">
            <button
              type="button"
              aria-expanded={open}
              aria-label={open ? `Hide ${server.name} details` : `Show ${server.name} details`}
              onClick={() => setOpen((o) => !o)}
              className="mt-0.5 rounded-md p-0.5 text-fg-muted hover:bg-hover hover:text-fg"
            >
              {open ? <ChevronDown className="size-4" /> : <ChevronRight className="size-4" />}
            </button>
            <div className="min-w-0 flex-1">
              <div className="flex flex-wrap items-center gap-2">
                <StatusDot status={server.status} />
                <h3 className="text-md font-medium text-fg">{server.name}</h3>
                <span className="text-sm text-fg-subtle" data-testid="mcp-status">
                  {STATUS_LABEL[server.status]}
                  {server.restarts > 0 && ` · restarted ${server.restarts}×`}
                </span>
                {server.transport === "http" && (
                  <span className="rounded-md bg-sunken px-1.5 py-0.5 text-xs text-fg-muted">
                    Remote
                  </span>
                )}
                {server.extension && (
                  <span className="rounded-md bg-sunken px-1.5 py-0.5 text-xs text-fg-muted">
                    Extension · {server.extension}
                  </span>
                )}
              </div>
              <p className="mt-0.5 truncate font-mono text-sm text-fg-muted" title={server.summary}>
                {server.summary}
              </p>
              {(counts || proto) && (
                <p className="mt-1 text-sm text-fg-subtle">
                  {[
                    counts,
                    proto,
                    server.serverName && `${server.serverName} ${server.serverVersion ?? ""}`,
                  ]
                    .filter(Boolean)
                    .join(" · ")}
                </p>
              )}
              {server.error && server.status !== "running" && (
                <p className="mt-1.5 text-sm text-danger">{server.error}</p>
              )}
            </div>
            <div className="flex shrink-0 items-center gap-2">
              {server.status === "needsAuth" && (
                <Button
                  primary
                  disabled={signIn.isPending}
                  onClick={() =>
                    void run(signIn.mutateAsync(server.name), `Signed in to ${server.name}.`)
                  }
                >
                  <LogIn />
                  {signIn.isPending ? "Waiting for the browser…" : "Sign in"}
                </Button>
              )}
              {enabled && server.status !== "needsAuth" && (
                <Button
                  label={`Restart ${server.name}`}
                  disabled={restart.isPending}
                  onClick={() => void run(restart.mutateAsync(server.name))}
                >
                  <RotateCw />
                </Button>
              )}
              {server.extension ? (
                <Button
                  label={`${server.name} settings in Extensions`}
                  onClick={() => openSettings("extensions")}
                >
                  Manage
                </Button>
              ) : (
                <Button label={`Edit ${server.name}`} onClick={() => setEditing(true)}>
                  Edit
                </Button>
              )}
              <Switch
                checked={enabled}
                label={`Use ${server.name}`}
                disabled={setEnabled.isPending}
                onChange={(on) =>
                  void run(setEnabled.mutateAsync({ name: server.name, enabled: on }))
                }
              />
            </div>
          </div>
          {open && (
            <div className="border-t border-line">
              <ServerDetails server={server} />
              {server.signedIn && (
                <div className="flex justify-end border-t border-line px-4 py-2">
                  <Button
                    onClick={() =>
                      void run(signOut.mutateAsync(server.name), `Signed out of ${server.name}.`)
                    }
                  >
                    <LogOut />
                    Sign out
                  </Button>
                </div>
              )}
            </div>
          )}
        </>
      )}
    </section>
  );
}

const EMPTY: McpServerConfig = {
  name: "",
  transport: "stdio",
  command: "",
  args: [],
  env: {},
  url: "",
  headers: {},
  enabled: true,
};

/** Add (no `name`) or edit a server. */
function ServerForm({ name, onDone }: { name?: string; onDone: () => void }) {
  const existing = useMcpServerConfig(name ?? null);
  if (name && existing.isPending) return <p className="text-fg-muted">Loading…</p>;
  if (name && existing.error) {
    return (
      <div className="flex items-center justify-between gap-4">
        <p className="text-danger">{errorMessage(existing.error)}</p>
        <Button onClick={onDone}>Close</Button>
      </div>
    );
  }
  return (
    <ServerFormFields initial={existing.data ?? EMPTY} oldName={name ?? null} onDone={onDone} />
  );
}

function ServerFormFields({
  initial,
  oldName,
  onDone,
}: {
  initial: McpServerConfig;
  oldName: string | null;
  onDone: () => void;
}) {
  const id = useId();
  const [name, setName] = useState(initial.name);
  const [transport, setTransport] = useState<McpTransport>(initial.transport);
  const [command, setCommand] = useState(initial.command);
  const [args, setArgs] = useState(initial.args.join("\n"));
  const [env, setEnv] = useState(formatPairs(initial.env, "="));
  const [url, setUrl] = useState(initial.url);
  const [headers, setHeaders] = useState(formatPairs(initial.headers, ":"));
  const [error, setError] = useState<string | null>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const save = useSaveMcpServer();
  const remove = useRemoveMcpServer();

  const onSave = async () => {
    setError(null);
    const server: McpServerConfig = {
      name: name.trim(),
      transport,
      command: transport === "stdio" ? command.trim() : "",
      args: transport === "stdio" ? parseArgs(args) : [],
      env: transport === "stdio" ? parsePairs(env, "=") : {},
      url: transport === "http" ? url.trim() : "",
      headers: transport === "http" ? parsePairs(headers, ":") : {},
      enabled: initial.enabled,
    };
    if (!server.name) return setError("Give the server a name.");
    if (transport === "stdio" && !server.command) return setError("Enter the command to run.");
    if (transport === "http" && !server.url) return setError("Enter the server's URL.");
    try {
      await save.mutateAsync({ server, oldName });
      toast(oldName ? `Saved ${server.name}.` : `Added ${server.name}.`);
      onDone();
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  return (
    <form
      aria-label={oldName ? `Edit ${oldName}` : "New server"}
      onSubmit={(e) => {
        e.preventDefault();
        void onSave();
      }}
      className="flex flex-col gap-4"
    >
      <FormRow label="Name" htmlFor={`${id}-name`}>
        <input
          id={`${id}-name`}
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="filesystem"
          className={inputClass}
        />
      </FormRow>
      <fieldset>
        <legend className="mb-1.5 text-sm font-medium text-fg-muted">Type</legend>
        <div className="flex gap-2">
          {(
            [
              ["stdio", "Local command"],
              ["http", "Remote URL"],
            ] as const
          ).map(([value, label]) => (
            <label
              key={value}
              className={cn(
                "flex h-9 cursor-pointer items-center gap-2 rounded-lg border px-3 text-base",
                transport === value
                  ? "border-accent bg-accent-soft text-fg"
                  : "border-line text-fg-muted",
              )}
            >
              <input
                type="radio"
                name={`${id}-transport`}
                value={value}
                checked={transport === value}
                onChange={() => setTransport(value)}
                className="sr-only"
              />
              {label}
            </label>
          ))}
        </div>
      </fieldset>
      {transport === "stdio" ? (
        <>
          <FormRow
            label="Command"
            htmlFor={`${id}-command`}
            hint="For example npx, uvx, node, python, docker, or a full path."
          >
            <input
              id={`${id}-command`}
              value={command}
              onChange={(e) => setCommand(e.target.value)}
              placeholder="npx"
              className={cn(inputClass, "font-mono text-sm")}
            />
          </FormRow>
          <FormRow label="Arguments" htmlFor={`${id}-args`} hint="One per line.">
            <textarea
              id={`${id}-args`}
              value={args}
              onChange={(e) => setArgs(e.target.value)}
              rows={3}
              placeholder={"-y\n@modelcontextprotocol/server-filesystem\nC:\\Users\\me\\Documents"}
              className={textareaClass}
            />
          </FormRow>
          <FormRow
            label="Environment variables"
            htmlFor={`${id}-env`}
            hint="One KEY=value per line. Stored in the config file as written."
          >
            <textarea
              id={`${id}-env`}
              value={env}
              onChange={(e) => setEnv(e.target.value)}
              rows={2}
              placeholder="API_TOKEN=…"
              className={textareaClass}
            />
          </FormRow>
        </>
      ) : (
        <>
          <FormRow
            label="URL"
            htmlFor={`${id}-url`}
            hint="The Streamable HTTP endpoint, often ending in /mcp. Servers that need a sign-in show a Sign in button."
          >
            <input
              id={`${id}-url`}
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              placeholder="https://example.com/mcp"
              className={cn(inputClass, "font-mono text-sm")}
            />
          </FormRow>
          <FormRow
            label="Headers"
            htmlFor={`${id}-headers`}
            hint="Optional. One Name: value per line, e.g. an API key the server expects."
          >
            <textarea
              id={`${id}-headers`}
              value={headers}
              onChange={(e) => setHeaders(e.target.value)}
              rows={2}
              placeholder="Authorization: Bearer …"
              className={textareaClass}
            />
          </FormRow>
        </>
      )}
      {error && (
        <p role="alert" className="text-base text-danger">
          {error}
        </p>
      )}
      <div className="flex items-center gap-2">
        <Button primary type="submit" disabled={save.isPending}>
          {oldName ? "Save" : "Add server"}
        </Button>
        <Button onClick={onDone}>Cancel</Button>
        <span className="flex-1" />
        {oldName &&
          (confirmDelete ? (
            <span className="flex items-center gap-2 text-base">
              Remove {oldName}?
              <Button
                danger
                onClick={() =>
                  void remove
                    .mutateAsync(oldName)
                    .then(() => toast(`Removed ${oldName}.`))
                    .catch((e: unknown) => setError(errorMessage(e)))
                }
              >
                Remove
              </Button>
              <Button onClick={() => setConfirmDelete(false)}>Keep</Button>
            </span>
          ) : (
            <Button danger onClick={() => setConfirmDelete(true)}>
              Remove
            </Button>
          ))}
      </div>
    </form>
  );
}

/**
 * The browser agent (Phase 13): Oxide's own MCP server, run per chat in the managed Python, so
 * it isn't an entry of the config file. Its switch is Settings → Browser's.
 */
function BuiltInBrowserServer() {
  const { data } = useSettings();
  const browser = (data ?? DEFAULT_SETTINGS).browser;
  const setBrowser = useSetBrowserSettings();
  const { data: status } = useBrowserStatus(browser.enabled);
  const openSettings = useOpenSettings();
  const state = !browser.enabled
    ? "Off"
    : status?.installing
      ? "Setting up"
      : status && !status.packages
        ? "Sets up the first time a chat uses it"
        : status && status.openChats.length > 0
          ? `Open in ${status.openChats.length} chat${status.openChats.length === 1 ? "" : "s"}`
          : "Starts with each chat's first browser call";
  return (
    <section
      aria-label="browser (built in)"
      className="flex items-start gap-3 rounded-xl border border-line bg-raised p-4"
    >
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <span className="text-md font-medium text-fg">browser</span>
          <span className="rounded bg-sunken px-1.5 text-xs text-fg-subtle">Built in</span>
        </div>
        <p className="mt-1 text-sm text-fg-subtle">
          Oxide&apos;s browser agent (Browser-Use and Playwright), run in Oxide&apos;s Python for
          each chat with a browser of its own. Its tools reach models as{" "}
          <span className="font-mono">browser__…</span> and go through the browser&apos;s approvals.{" "}
          {state}.
        </p>
      </div>
      <div className="flex items-center gap-2">
        <Button onClick={() => openSettings("browser")}>Browser settings</Button>
        <Switch
          checked={browser.enabled}
          label="Use browser"
          onChange={(enabled) => setBrowser({ enabled })}
        />
      </div>
    </section>
  );
}

/**
 * The sandbox's file server (Phase 12b): Oxide's own, run per chat from its Docker image, so
 * it isn't an entry of the config file. Shown here so people can see where those tools come
 * from.
 */
function BuiltInSandboxServer() {
  const { data } = useSettings();
  const sandbox = (data ?? DEFAULT_SETTINGS).sandbox;
  const { data: docker } = useDockerStatus(sandbox.enabled);
  const openSettings = useOpenSettings();
  if (!sandbox.enabled) return null;
  const image = docker?.images.find((i) => i.role === "Files");
  const status =
    docker?.state !== "ready"
      ? "Docker isn't running"
      : image && !image.present
        ? "Image not downloaded yet"
        : "Starts with each chat's sandbox";
  return (
    <section
      aria-label="sandbox (built in)"
      className="flex items-start gap-3 rounded-xl border border-line bg-raised p-4"
    >
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <span className="text-md font-medium text-fg">sandbox</span>
          <span className="rounded bg-sunken px-1.5 text-xs text-fg-subtle">Built in</span>
        </div>
        <p className="mt-1 text-sm text-fg-subtle">
          The reference filesystem server (<span className="font-mono">{sandbox.filesImage}</span>
          ), run in Docker for each chat with only that chat&apos;s sandbox workspace. Its tools
          reach models as <span className="font-mono">sandbox__…</span> and go through the
          sandbox&apos;s approvals. {status}.
        </p>
      </div>
      <Button onClick={() => openSettings("sandbox")}>Sandbox settings</Button>
    </section>
  );
}
