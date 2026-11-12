import type {
  ChatSandbox,
  CommandError,
  DockerStatus,
  Result,
  SandboxChange,
  SandboxFile,
  commands as RealCommands,
} from "@/bindings";

/**
 * The Docker sandbox in the fake backend. Docker is running with both images downloaded
 * unless a test says otherwise (`setFakeDocker`); chats' sandboxes start as "none" and their
 * files are whatever a test puts there (`setFakeSandbox`).
 */
interface FakeSandbox {
  docker: DockerStatus;
  chats: Map<string, ChatSandbox>;
  /** Files per chat, per folder path. */
  files: Map<string, Map<string, SandboxFile[]>>;
  prepared: number;
  /** What the next file or folder picker returns (`null`: cancelled). */
  nextPick: string | null;
  copiedIn: { conversationId: string; dest: string; folder: boolean; picked: string }[];
  saved: { conversationId: string; path: string }[];
  stops: string[];
  resets: string[];
}

let sb: FakeSandbox;
let emit: (c: SandboxChange) => void = () => {};

export function setSandboxEmitter(fn: (c: SandboxChange) => void): void {
  emit = fn;
}

function readyDocker(): DockerStatus {
  return {
    state: "ready",
    version: "28.1.1",
    message: null,
    program: "C:/Program Files/Docker/Docker/resources/bin/docker.exe",
    cpus: 8,
    memoryBytes: 16 * 1024 * 1024 * 1024,
    images: [
      { name: "python:3.12-bookworm", role: "Commands", present: true, sizeBytes: 1_020_000_000 },
      { name: "mcp/filesystem", role: "Files", present: true, sizeBytes: 160_000_000 },
    ],
    pulling: null,
    installUrl: "https://docs.docker.com/get-started/get-docker/",
    checkedAt: 0,
  };
}

function noSandbox(conversationId: string): ChatSandbox {
  return {
    conversationId,
    state: "none",
    container: `oxide-sandbox-${conversationId}`,
    image: null,
    workspace: null,
    storage: "volume",
    network: false,
    commands: [],
    error: null,
  };
}

export function resetFakeSandbox(): void {
  sb = {
    docker: readyDocker(),
    chats: new Map(),
    files: new Map(),
    prepared: 0,
    nextPick: null,
    copiedIn: [],
    saved: [],
    stops: [],
    resets: [],
  };
}
resetFakeSandbox();

export function fakeSandboxState(): Readonly<FakeSandbox> {
  return sb;
}

/** Changes Docker's status and sends the event. */
export function setFakeDocker(patch: Partial<DockerStatus>): void {
  sb.docker = { ...sb.docker, ...patch };
  emit({ conversationId: null });
}

/** Docker isn't installed. */
export function fakeDockerMissing(): void {
  setFakeDocker({
    state: "missing",
    version: null,
    cpus: null,
    memoryBytes: null,
    program: null,
    images: [],
    message: "Docker isn't installed (or Oxide can't find the docker program).",
  });
}

/** Sets a chat's sandbox (and, optionally, its files per folder) and sends the event. */
export function setFakeSandbox(
  conversationId: string,
  patch: Partial<ChatSandbox>,
  files?: Record<string, SandboxFile[]>,
): void {
  const cur = sb.chats.get(conversationId) ?? noSandbox(conversationId);
  sb.chats.set(conversationId, { ...cur, ...patch });
  if (files) sb.files.set(conversationId, new Map(Object.entries(files)));
  emit({ conversationId });
}

/** What the next picker returns. */
export function setFakePick(path: string | null): void {
  sb.nextPick = path;
}

/** A file entry for `setFakeSandbox`. */
export function sandboxFile(path: string, size: number | null = 10, dir = false): SandboxFile {
  return { name: path.split("/").pop() ?? path, path, dir, link: false, size: dir ? null : size };
}

const ok = <T>(data: T): Promise<Result<T, CommandError>> =>
  Promise.resolve({ status: "ok", data: JSON.parse(JSON.stringify(data)) as T });
const fail = (message: string): Promise<Result<never, CommandError>> =>
  Promise.resolve({ status: "error", error: { kind: "invalid", message } });

function chat(id: string): ChatSandbox {
  return sb.chats.get(id) ?? noSandbox(id);
}

export const fakeSandboxCommands: Pick<
  typeof RealCommands,
  | "sandboxDockerStatus"
  | "sandboxPrepare"
  | "chatSandbox"
  | "sandboxFiles"
  | "sandboxStop"
  | "sandboxReset"
  | "sandboxCopyIn"
  | "sandboxSave"
> = {
  sandboxDockerStatus: () => ok({ ...sb.docker, checkedAt: Date.now() }),
  sandboxPrepare: () => {
    if (sb.docker.state !== "ready") return fail("Docker isn't running.");
    sb.prepared += 1;
    sb.docker = { ...sb.docker, images: sb.docker.images.map((i) => ({ ...i, present: true })) };
    emit({ conversationId: null });
    return ok(null);
  },
  chatSandbox: (id) => {
    const c = chat(id);
    if (sb.docker.state !== "ready" && c.state === "none") {
      return ok({ ...c, error: sb.docker.message });
    }
    return ok(c);
  },
  sandboxFiles: (id, path, start) => {
    const c = chat(id);
    if (c.state !== "running") {
      if (!start) return ok(null);
      sb.chats.set(id, { ...c, state: "running" });
      emit({ conversationId: id });
    }
    const entries = sb.files.get(id)?.get(path) ?? [];
    return ok({ path, entries, truncated: false });
  },
  sandboxStop: (id) => {
    sb.stops.push(id);
    sb.chats.set(id, { ...chat(id), state: "stopped" });
    emit({ conversationId: id });
    return ok(null);
  },
  sandboxReset: (id) => {
    sb.resets.push(id);
    sb.chats.delete(id);
    sb.files.delete(id);
    emit({ conversationId: id });
    return ok(null);
  },
  sandboxCopyIn: (id, dest, folder) => {
    const picked = sb.nextPick;
    if (picked === null) return ok(null);
    sb.copiedIn.push({ conversationId: id, dest, folder, picked });
    const name = picked.split(/[\\/]/).pop() ?? picked;
    const at = `${dest.replace(/\/$/, "")}/${name}`;
    const folderPath = dest.replace(/\/$/, "");
    const byPath = sb.files.get(id) ?? new Map<string, SandboxFile[]>();
    byPath.set(folderPath, [...(byPath.get(folderPath) ?? []), sandboxFile(at, 5, folder)]);
    sb.files.set(id, byPath);
    emit({ conversationId: id });
    return ok(at);
  },
  sandboxSave: (id, path) => {
    const picked = sb.nextPick;
    if (picked === null) return ok(null);
    sb.saved.push({ conversationId: id, path });
    return ok(`${picked}\\${path.split("/").pop() ?? path}`);
  },
};
