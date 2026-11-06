import { ExternalLink, Plug } from "lucide-react";
import { Dialog } from "radix-ui";
import { useId, useMemo, useState } from "react";
import { commands, type McpAnswer, type McpInteraction } from "@/bindings";
import { allModels, isUsable } from "@/features/models/models";
import { useModels } from "@/features/models/queries";
import { openExternal } from "@/lib/links";
import { cn } from "@/lib/cn";
import { formContent, formFields, type FormField } from "./schema";
import { useMcpUi } from "./store";
import { Button, inputClass } from "./ui";

/**
 * A server is asking the user something while it works: fill in a form, open a link, or let
 * it use a model (sampling). One question at a time, oldest first. Mounted in `AppLayout`.
 */
export function McpAskDialog() {
  const ask = useMcpUi((s) => s.asks[0]);
  if (!ask) return null;
  return <AskDialog key={ask.id} interaction={ask} />;
}

function AskDialog({ interaction }: { interaction: McpInteraction }) {
  const closeAsk = useMcpUi((s) => s.closeAsk);
  const answer = (a: McpAnswer) => {
    void commands.mcpAnswer(interaction.id, a).catch(() => {});
    closeAsk(interaction.id);
  };
  const ask = interaction.ask;
  const dismiss = () =>
    answer(
      ask.kind === "elicitation"
        ? { kind: "elicitation", action: "cancel", content: null }
        : { kind: "sampling", approved: false, model: null },
    );

  return (
    <Dialog.Root open onOpenChange={(o) => !o && dismiss()}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-[70] bg-overlay" />
        <Dialog.Content className="ox-pop fixed left-1/2 top-1/2 z-[70] flex max-h-[min(620px,calc(100vh-64px))] w-[min(520px,calc(100vw-48px))] -translate-x-1/2 -translate-y-1/2 flex-col rounded-2xl border border-line bg-app p-6 shadow-lg outline-none">
          <p className="mb-1 flex items-center gap-1.5 text-sm text-fg-subtle">
            <Plug className="size-3.5" />
            {interaction.server}
          </p>
          {ask.kind === "elicitation" ? (
            ask.url ? (
              <UrlAsk message={ask.message} url={ask.url} onAnswer={answer} />
            ) : (
              <FormAsk message={ask.message} schema={ask.schema} onAnswer={answer} />
            )
          ) : (
            <SamplingAsk
              server={interaction.server}
              systemPrompt={ask.systemPrompt}
              messages={ask.messages.map((m) => ({
                role: m.role,
                text: m.content.type === "text" ? m.content.text : `[${m.content.type}]`,
              }))}
              maxTokens={ask.maxTokens}
              hints={ask.modelHints}
              onAnswer={answer}
            />
          )}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

type Answer = (a: McpAnswer) => void;

function FormAsk({
  message,
  schema,
  onAnswer,
}: {
  message: string;
  schema: string | null;
  onAnswer: Answer;
}) {
  const fields = useMemo(() => formFields(schema), [schema]);
  const [values, setValues] = useState<Record<string, string | boolean | string[]>>(() =>
    Object.fromEntries(
      fields
        .filter((f) => f.defaultValue !== undefined && f.defaultValue !== null)
        .map((f) => [
          f.name,
          typeof f.defaultValue === "boolean"
            ? f.defaultValue
            : Array.isArray(f.defaultValue)
              ? f.defaultValue.map(String)
              : String(f.defaultValue),
        ]),
    ),
  );
  const [error, setError] = useState<string | null>(null);

  const submit = () => {
    const r = formContent(fields, values);
    if ("error" in r) {
      setError(r.error);
      return;
    }
    onAnswer({ kind: "elicitation", action: "accept", content: JSON.stringify(r.content) });
  };

  return (
    <form
      onSubmit={(e) => {
        e.preventDefault();
        submit();
      }}
      className="flex min-h-0 flex-col"
    >
      <Dialog.Title className="mb-1 font-serif text-xl font-normal text-fg">
        The server needs some information
      </Dialog.Title>
      <Dialog.Description className="mb-4 whitespace-pre-wrap text-base text-fg-muted">
        {message}
      </Dialog.Description>
      <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto">
        {fields.map((f) => (
          <Field
            key={f.name}
            field={f}
            value={values[f.name]}
            onChange={(v) => setValues((prev) => ({ ...prev, [f.name]: v }))}
          />
        ))}
      </div>
      {error && (
        <p role="alert" className="mt-3 text-base text-danger">
          {error}
        </p>
      )}
      <p className="mt-4 text-xs text-fg-subtle">
        Only share what you’re comfortable sending to this server.
      </p>
      <div className="mt-4 flex justify-end gap-2">
        <Button onClick={() => onAnswer({ kind: "elicitation", action: "decline", content: null })}>
          Decline
        </Button>
        <Button primary type="submit">
          Send
        </Button>
      </div>
    </form>
  );
}

function Field({
  field,
  value,
  onChange,
}: {
  field: FormField;
  value: string | boolean | string[] | undefined;
  onChange: (v: string | boolean | string[]) => void;
}) {
  const id = useId();
  const label = (
    <label htmlFor={id} className="mb-1 block text-sm font-medium text-fg-muted">
      {field.label}
      {field.required && " *"}
    </label>
  );
  const hint = field.description && (
    <p className="mt-1 text-sm text-fg-subtle">{field.description}</p>
  );
  if (field.kind === "boolean") {
    return (
      <div>
        <label className="flex items-center gap-2 text-base text-fg">
          <input
            id={id}
            type="checkbox"
            className="accent-accent"
            checked={value === true}
            onChange={(e) => onChange(e.target.checked)}
          />
          {field.label}
        </label>
        {hint}
      </div>
    );
  }
  if (field.kind === "select") {
    return (
      <div>
        {label}
        <select
          id={id}
          value={typeof value === "string" ? value : ""}
          onChange={(e) => onChange(e.target.value)}
          className={inputClass}
        >
          <option value="">Choose…</option>
          {field.options?.map((o) => (
            <option key={o.value} value={o.value}>
              {o.label}
            </option>
          ))}
        </select>
        {hint}
      </div>
    );
  }
  if (field.kind === "multiselect") {
    const chosen = Array.isArray(value) ? value : [];
    return (
      <fieldset>
        <legend className="mb-1 text-sm font-medium text-fg-muted">
          {field.label}
          {field.required && " *"}
        </legend>
        {field.options?.map((o) => (
          <label key={o.value} className="flex items-center gap-2 text-base text-fg">
            <input
              type="checkbox"
              className="accent-accent"
              checked={chosen.includes(o.value)}
              onChange={(e) =>
                onChange(
                  e.target.checked ? [...chosen, o.value] : chosen.filter((c) => c !== o.value),
                )
              }
            />
            {o.label}
          </label>
        ))}
        {hint}
      </fieldset>
    );
  }
  const type =
    field.kind === "number" || field.kind === "integer"
      ? "number"
      : field.format === "email"
        ? "email"
        : field.format === "uri"
          ? "url"
          : field.format === "date"
            ? "date"
            : "text";
  return (
    <div>
      {label}
      <input
        id={id}
        type={type}
        step={field.kind === "integer" ? 1 : undefined}
        min={field.min}
        max={field.max}
        value={typeof value === "string" ? value : ""}
        onChange={(e) => onChange(e.target.value)}
        className={inputClass}
      />
      {hint}
    </div>
  );
}

function UrlAsk({ message, url, onAnswer }: { message: string; url: string; onAnswer: Answer }) {
  const host = (() => {
    try {
      return new URL(url).host;
    } catch {
      return url;
    }
  })();
  return (
    <>
      <Dialog.Title className="mb-1 font-serif text-xl font-normal text-fg">
        Continue in your browser
      </Dialog.Title>
      <Dialog.Description className="mb-3 whitespace-pre-wrap text-base text-fg-muted">
        {message}
      </Dialog.Description>
      <p className="mb-4 break-all rounded-lg bg-sunken px-3 py-2 font-mono text-sm text-fg">
        {url}
      </p>
      <p className="text-xs text-fg-subtle">
        This opens <span className="font-medium">{host}</span>. Check that you trust it first.
      </p>
      <div className="mt-5 flex justify-end gap-2">
        <Button onClick={() => onAnswer({ kind: "elicitation", action: "decline", content: null })}>
          Decline
        </Button>
        <Button
          primary
          onClick={() => {
            openExternal(url);
            onAnswer({ kind: "elicitation", action: "accept", content: null });
          }}
        >
          <ExternalLink />
          Open
        </Button>
      </div>
    </>
  );
}

function SamplingAsk({
  server,
  systemPrompt,
  messages,
  maxTokens,
  hints,
  onAnswer,
}: {
  server: string;
  systemPrompt: string | null;
  messages: Array<{ role: string; text: string }>;
  maxTokens: number;
  hints: string[];
  onAnswer: Answer;
}) {
  const id = useId();
  const { data: lists } = useModels();
  const models = allModels(lists).filter(isUsable);
  // The first listed model whose name matches one of the server's hints.
  const hinted = hints
    .map((h) => h.toLowerCase())
    .flatMap((h) => models.filter((m) => m.model.toLowerCase().includes(h)))[0];
  const [model, setModel] = useState<string>("");
  const chosen = model || hinted?.id || "";

  return (
    <>
      <Dialog.Title className="mb-1 font-serif text-xl font-normal text-fg">
        Let {server} use a model?
      </Dialog.Title>
      <Dialog.Description className="mb-3 text-base text-fg-muted">
        The server wants a model to answer the conversation below. It sees the answer; nothing else
        from your chats is shared.
      </Dialog.Description>
      <div className="min-h-0 flex-1 overflow-y-auto rounded-lg border border-line bg-raised p-3 text-sm">
        {systemPrompt && (
          <p className="mb-2 whitespace-pre-wrap text-fg-subtle">
            <span className="font-medium">System: </span>
            {systemPrompt}
          </p>
        )}
        {messages.map((m, i) => (
          <p
            key={i}
            className={cn(
              "mb-2 whitespace-pre-wrap",
              m.role === "user" ? "text-fg" : "text-fg-muted",
            )}
          >
            <span className="font-medium capitalize">{m.role}: </span>
            {m.text}
          </p>
        ))}
        <p className="text-xs text-fg-subtle">Up to {maxTokens.toLocaleString()} tokens.</p>
      </div>
      <label htmlFor={`${id}-model`} className="mb-1 mt-4 block text-sm font-medium text-fg-muted">
        Model
      </label>
      <select
        id={`${id}-model`}
        value={chosen}
        onChange={(e) => setModel(e.target.value)}
        className={inputClass}
      >
        <option value="">Default model</option>
        {models.map((m) => (
          <option key={m.id} value={m.id}>
            {m.label}
          </option>
        ))}
      </select>
      <div className="mt-5 flex justify-end gap-2">
        <Button onClick={() => onAnswer({ kind: "sampling", approved: false, model: null })}>
          Deny
        </Button>
        <Button
          primary
          onClick={() => onAnswer({ kind: "sampling", approved: true, model: chosen || null })}
        >
          Allow
        </Button>
      </div>
    </>
  );
}
