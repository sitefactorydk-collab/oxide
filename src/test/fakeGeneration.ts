import type {
  CommandError,
  GenerationEndpoint,
  GenerationKind,
  Result,
  commands as RealCommands,
} from "@/bindings";

/**
 * Settings → Generation and generated media in the fake backend (Phase 19). Keys of their
 * own are kept in `fakeGeneration().keys`; Test passes when the endpoint lists the model in
 * `models`; `attachmentMedia` serves `media` by path; downloads land in `saved`.
 */
interface FakeGeneration {
  keys: Map<GenerationKind, string>;
  models: string[];
  tests: Array<{ kind: GenerationKind; endpoint: GenerationEndpoint }>;
  media: Map<string, string>;
  saved: Array<{ path: string; name: string }>;
}

let f: FakeGeneration;

export function resetFakeGeneration(): void {
  f = {
    keys: new Map(),
    models: ["gpt-image-2", "gpt-4o-mini-tts", "sora-2"],
    tests: [],
    media: new Map(),
    saved: [],
  };
}
resetFakeGeneration();

export function fakeGeneration(): Readonly<FakeGeneration> {
  return f;
}

/** Bytes (base64) a stored file serves to a player. */
export function setFakeMedia(path: string, base64: string): void {
  f.media.set(path, base64);
}

const ok = <T>(data: T): Promise<Result<T, CommandError>> =>
  Promise.resolve({ status: "ok", data });
const fail = (message: string): Promise<Result<never, CommandError>> =>
  Promise.resolve({ status: "error", error: { kind: "invalid", message } });

const reachable = (e: GenerationEndpoint) => e.providerId !== null || e.baseUrl !== "";

export const fakeGenerationCommands: Pick<
  typeof RealCommands,
  | "generationKeys"
  | "setGenerationKey"
  | "testGeneration"
  | "generationModels"
  | "attachmentMedia"
  | "saveAttachmentFile"
> = {
  generationKeys: () => ok([...f.keys.keys()]),
  setGenerationKey: (kind, key) => {
    if (key && key.trim() !== "") f.keys.set(kind, key.trim());
    else f.keys.delete(kind);
    return ok(null);
  },
  testGeneration: (kind, endpoint) => {
    f.tests.push({ kind, endpoint });
    if (!reachable(endpoint)) {
      return ok({ ok: false, message: `No endpoint is set for ${kind} generation.` });
    }
    return ok(
      f.models.includes(endpoint.model)
        ? { ok: true, message: `Connected. ${endpoint.model} is offered.` }
        : { ok: true, message: `Connected, but ${endpoint.model} isn't among the models.` },
    );
  },
  generationModels: (_kind, endpoint) =>
    reachable(endpoint) ? ok(f.models) : fail("No endpoint is set."),
  attachmentMedia: (path) => {
    const data = f.media.get(path);
    return data === undefined
      ? fail("an attachment isn't in Oxide's attachments folder")
      : ok(data);
  },
  saveAttachmentFile: (path, name) => {
    f.saved.push({ path, name });
    return ok(`/Users/test/Downloads/${name}`);
  },
};
