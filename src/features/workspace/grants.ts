/** A grant key as people read it. */
export function grantLabel(key: string): string {
  if (key.startsWith("builtin/run_command:")) {
    return `Command: ${key.slice("builtin/run_command:".length)}`;
  }
  if (key.startsWith("builtin/sandbox__")) {
    return `Sandbox files: ${key.slice("builtin/sandbox__".length).replace(/_/g, " ")}`;
  }
  if (key.startsWith("builtin/")) return key.slice("builtin/".length).replace(/_/g, " ");
  const [server, ...rest] = key.split("/");
  return `${rest.join("/")} · ${server}`;
}
