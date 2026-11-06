import { Dialog } from "radix-ui";
import { useId, useState } from "react";
import type { McpPrompt } from "@/bindings";
import { Button, inputClass } from "./ui";

/** Fills in an MCP prompt's arguments before it's attached to the message. */
export function PromptArgsDialog({
  prompt,
  onSubmit,
  onClose,
}: {
  prompt: McpPrompt;
  onSubmit: (args: Record<string, string>) => void;
  onClose: () => void;
}) {
  const id = useId();
  const [values, setValues] = useState<Record<string, string>>({});
  const [error, setError] = useState<string | null>(null);
  const submit = () => {
    const missing = prompt.arguments.find((a) => a.required && !values[a.name]?.trim());
    if (missing) {
      setError(`${missing.title ?? missing.name} is required.`);
      return;
    }
    const args = Object.fromEntries(Object.entries(values).filter(([, v]) => v.trim().length > 0));
    onSubmit(args);
  };
  return (
    <Dialog.Root open onOpenChange={(o) => !o && onClose()}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-[60] bg-overlay" />
        <Dialog.Content className="ox-pop fixed left-1/2 top-1/2 z-[60] w-[min(460px,calc(100vw-48px))] -translate-x-1/2 -translate-y-1/2 rounded-2xl border border-line bg-app p-6 shadow-lg outline-none">
          <Dialog.Title className="font-serif text-xl font-normal text-fg">
            {prompt.title ?? prompt.name}
          </Dialog.Title>
          <Dialog.Description className="mb-4 mt-1 text-base text-fg-muted">
            {prompt.description ?? `A prompt from ${prompt.server}.`}
          </Dialog.Description>
          <form
            onSubmit={(e) => {
              e.preventDefault();
              submit();
            }}
            className="flex flex-col gap-3"
          >
            {prompt.arguments.map((a, i) => (
              <div key={a.name}>
                <label
                  htmlFor={`${id}-${a.name}`}
                  className="mb-1 block text-sm font-medium text-fg-muted"
                >
                  {a.title ?? a.name}
                  {a.required && " *"}
                </label>
                <input
                  id={`${id}-${a.name}`}
                  autoFocus={i === 0}
                  value={values[a.name] ?? ""}
                  placeholder={a.description ?? ""}
                  onChange={(e) => setValues((v) => ({ ...v, [a.name]: e.target.value }))}
                  className={inputClass}
                />
              </div>
            ))}
            {error && (
              <p role="alert" className="text-base text-danger">
                {error}
              </p>
            )}
            <div className="mt-2 flex justify-end gap-2">
              <Button onClick={onClose}>Cancel</Button>
              <Button primary type="submit">
                Add to message
              </Button>
            </div>
          </form>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
