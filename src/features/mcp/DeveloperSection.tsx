import { ExternalLink, Save } from "lucide-react";
import { useEffect, useId, useState } from "react";
import { commands } from "@/bindings";
import { errorMessage, unwrap } from "@/lib/api";
import { cn } from "@/lib/cn";
import { toast } from "@/lib/toast";
import { useMcpConfig, useMcpLogs, useMcpServers, useSaveMcpConfig } from "./queries";
import { LogLines } from "./ServerDetails";
import { Button, StatusDot, textareaClass } from "./ui";

/** Settings → Developer: edit `oxide_config.json` directly, and read every server's log. */
export function DeveloperSection() {
  return (
    <div className="flex max-w-3xl flex-col gap-10">
      <ConfigEditor />
      <ServerLogs />
    </div>
  );
}

function ConfigEditor() {
  const id = useId();
  const config = useMcpConfig();
  const save = useSaveMcpConfig();
  const [text, setText] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const loaded = config.data?.file.text ?? "";
  const value = text ?? loaded;
  const dirty = text !== null && text !== loaded;

  // Show edits made in another editor, unless the user is typing here.
  useEffect(() => {
    if (!dirty) setText(null);
  }, [loaded, dirty]);

  const onSave = async () => {
    setError(null);
    try {
      JSON.parse(value);
    } catch (e) {
      setError(`That isn't valid JSON: ${errorMessage(e)}`);
      return;
    }
    try {
      await save.mutateAsync(value);
      setText(null);
      toast("Saved. Servers restart where their entries changed.");
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  return (
    <section aria-labelledby={`${id}-title`}>
      <h3 id={`${id}-title`} className="mb-1 text-md font-medium text-fg">
        MCP config file
      </h3>
      <p className="mb-3 text-sm text-fg-subtle">
        <span className="break-all font-mono">{config.data?.file.path ?? "oxide_config.json"}</span>
        . Its <span className="font-mono">mcpServers</span> block uses Claude desktop’s format.
        Oxide-only settings go under a top-level <span className="font-mono">oxide</span> key.
        Changes made in another editor apply within a second.
      </p>
      {config.data?.error && (
        <p role="alert" className="mb-3 rounded-lg bg-danger-soft px-3 py-2 text-base text-fg">
          {config.data.error}
        </p>
      )}
      <label htmlFor={`${id}-text`} className="sr-only">
        Config file contents
      </label>
      <textarea
        id={`${id}-text`}
        value={config.isPending ? "" : value}
        onChange={(e) => setText(e.target.value)}
        spellCheck={false}
        rows={16}
        className={cn(textareaClass, "min-h-64")}
      />
      {error && (
        <p role="alert" className="mt-2 text-base text-danger">
          {error}
        </p>
      )}
      <div className="mt-2 flex gap-2">
        <Button primary disabled={!dirty || save.isPending} onClick={() => void onSave()}>
          <Save />
          Save
        </Button>
        <Button disabled={!dirty} onClick={() => setText(null)}>
          Discard changes
        </Button>
        <span className="flex-1" />
        <Button
          onClick={() =>
            void unwrap(commands.mcpOpenConfig()).catch((e: unknown) =>
              toast(errorMessage(e), "error"),
            )
          }
        >
          <ExternalLink />
          Open in editor
        </Button>
      </div>
    </section>
  );
}

function ServerLogs() {
  const id = useId();
  const { data: servers } = useMcpServers();
  const [chosen, setChosen] = useState<string | null>(null);
  const name = chosen ?? servers[0]?.name ?? null;
  return (
    <section aria-labelledby={`${id}-title`}>
      <h3 id={`${id}-title`} className="mb-1 text-md font-medium text-fg">
        Server logs
      </h3>
      <p className="mb-3 text-sm text-fg-subtle">
        What each server wrote to its error output, its log messages, and Oxide’s notes. Also saved
        in Oxide’s log folder as <span className="font-mono">mcp-&lt;name&gt;.log</span>.
      </p>
      {servers.length === 0 ? (
        <p className="text-base text-fg-muted">No servers are configured.</p>
      ) : (
        <>
          <div className="mb-2 flex flex-wrap gap-1" role="tablist" aria-label="Servers">
            {servers.map((s) => (
              <button
                key={s.name}
                type="button"
                role="tab"
                aria-selected={s.name === name}
                onClick={() => setChosen(s.name)}
                className={cn(
                  "flex h-8 items-center gap-2 rounded-lg px-3 text-base",
                  s.name === name ? "bg-active text-fg" : "text-fg-muted hover:bg-hover",
                )}
              >
                <StatusDot status={s.status} />
                {s.name}
              </button>
            ))}
          </div>
          {name && <ServerLog key={name} name={name} />}
        </>
      )}
    </section>
  );
}

function ServerLog({ name }: { name: string }) {
  const { data, isPending } = useMcpLogs(name);
  return <LogLines lines={data} loading={isPending} />;
}
