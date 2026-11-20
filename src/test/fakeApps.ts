import type {
  AppBundle,
  AppCallError,
  AppCapability,
  AppFile,
  AppImportPreview,
  AppManifest,
  AppPrefs,
  CommandError,
  GenerationKind,
  Result,
  UtilityApp,
  commands as RealCommands,
} from "@/bindings";

/**
 * Utility Apps in the fake backend (Phase 20). Apps live in `fakeApps().apps`; SDK calls
 * check the manifest and grants like the backend, then answer from `fakeApps().answer`
 * (default: echo the method and params). Calls are recorded in `calls`.
 */
interface FakeApps {
  apps: Map<string, AppBundle>;
  prefs: Map<string, AppPrefs>;
  keys: Map<string, Set<GenerationKind>>;
  storage: Map<string, number>;
  calls: Array<{ appId: string; method: string; params: unknown }>;
  answer: (appId: string, method: string, params: unknown) => unknown;
  exported: string[];
  /** What the next import pick returns (`null`: cancelled). */
  nextImport: AppBundle | null;
  imports: Map<string, AppBundle>;
}

const NEEDS: Record<string, AppCapability> = {
  "text.generate": "text",
  "media.image": "image",
  "media.video": "video",
  "media.music": "music",
  "web.search": "webSearch",
  "web.fetch": "webFetch",
  "web.crawl": "webFetch",
  "connectors.list": "connectors",
  "connectors.call": "connectors",
  "python.run": "python",
  "sandbox.exec": "sandbox",
};

export function fakeManifest(over: Partial<AppManifest> & { id: string }): AppManifest {
  return {
    name: over.id,
    description: "",
    icon: "sparkles",
    category: "Other",
    version: "1.0.0",
    entry: "App.tsx",
    capabilities: [],
    models: { text: null, image: null, video: null, music: null },
    author: null,
    ...over,
  };
}

export function fakeBundle(
  manifest: AppManifest,
  builtin: boolean,
  files: AppFile[] = [{ path: "App.tsx", content: "export default () => null;" }],
): AppBundle {
  return {
    app: { manifest, builtin, folder: builtin ? null : `/data/apps/${manifest.id}`, problem: null },
    files: [{ path: "oxide-app.json", content: JSON.stringify(manifest) }, ...files],
  };
}

function defaults(): Map<string, AppBundle> {
  const list: AppManifest[] = [
    fakeManifest({
      id: "ai-meta-tag-generator",
      name: "AI Meta Tag Generator",
      description: "Meta titles and descriptions.",
      icon: "search",
      category: "SEO",
      capabilities: ["text", "webFetch"],
    }),
    fakeManifest({
      id: "readability-tone-analyzer",
      name: "Readability & Tone Analyzer",
      description: "Readability and tone.",
      icon: "book-open",
      category: "Content",
      capabilities: ["text", "webFetch"],
    }),
    fakeManifest({
      id: "ai-html-table-from-image",
      name: "AI HTML Table from Image",
      description: "Tables from images.",
      icon: "table",
      category: "Code",
      capabilities: ["text"],
    }),
    fakeManifest({
      id: "ai-svg-creator",
      name: "AI SVG Creator",
      description: "SVGs from descriptions.",
      icon: "pen-tool",
      category: "Design",
      capabilities: ["text"],
    }),
  ];
  return new Map(list.map((m) => [m.id, fakeBundle(m, true)]));
}

let f: FakeApps;

export function resetFakeApps(): void {
  f = {
    apps: defaults(),
    prefs: new Map(),
    keys: new Map(),
    storage: new Map(),
    calls: [],
    answer: (_appId, method, params) => ({ method, params }),
    exported: [],
    nextImport: null,
    imports: new Map(),
  };
}
resetFakeApps();

export function fakeApps(): FakeApps {
  return f;
}

const ok = <T>(data: T): Promise<Result<T, CommandError>> =>
  Promise.resolve({ status: "ok", data });
const fail = (message: string): Promise<Result<never, CommandError>> =>
  Promise.resolve({ status: "error", error: { kind: "invalid", message } });

const emptyPrefs = (): AppPrefs => ({
  textModel: null,
  image: { providerId: null, baseUrl: "", model: "" },
  video: { providerId: null, baseUrl: "", model: "" },
  music: { providerId: null, baseUrl: "", model: "" },
  granted: [],
});
const prefsOf = (id: string) => f.prefs.get(id) ?? emptyPrefs();

function list(): UtilityApp[] {
  const all = [...f.apps.values()].map((b) => b.app);
  return [
    ...all.filter((a) => a.builtin),
    ...all.filter((a) => !a.builtin).sort((a, b) => a.manifest.name.localeCompare(b.manifest.name)),
  ];
}

export const fakeAppCommands: Pick<
  typeof RealCommands,
  | "listApps"
  | "appBundle"
  | "cloneApp"
  | "deleteApp"
  | "saveAppFile"
  | "exportApp"
  | "pickAppImport"
  | "installAppImport"
  | "appPrefs"
  | "saveAppPrefs"
  | "grantAppCapability"
  | "setAppKey"
  | "clearAppStorage"
  | "appCall"
  | "cancelAppCall"
  | "saveAppOutput"
> = {
  listApps: () => ok(list()),
  appBundle: (id) => {
    const b = f.apps.get(id);
    return b ? ok(b) : fail("app not found");
  },
  cloneApp: (id) => {
    const b = f.apps.get(id);
    if (!b) return fail("app not found");
    let newId = `${id}-copy`;
    for (let n = 2; f.apps.has(newId); n++) newId = `${id}-copy-${n}`;
    const manifest = { ...b.app.manifest, id: newId, name: `${b.app.manifest.name} (copy)` };
    const copy = fakeBundle(
      manifest,
      false,
      b.files.filter((x) => x.path !== "oxide-app.json"),
    );
    f.apps.set(newId, copy);
    return ok(copy.app);
  },
  deleteApp: (id) => {
    const b = f.apps.get(id);
    if (!b || b.app.builtin) return fail("Built-in apps can't be deleted.");
    f.apps.delete(id);
    f.prefs.delete(id);
    return ok(null);
  },
  saveAppFile: (id, path, content) => {
    const b = f.apps.get(id);
    if (!b || b.app.builtin) return fail("Built-in apps can't be changed. Clone the app first.");
    const files = b.files.filter((x) => x.path !== path).concat({ path, content });
    f.apps.set(id, { ...b, files });
    return ok(b.app);
  },
  exportApp: (id) => {
    f.exported.push(id);
    return ok(`/Users/test/Downloads/${id}.oxideapp`);
  },
  pickAppImport: () => {
    const b = f.nextImport;
    if (!b) return ok(null);
    const token = `t${f.imports.size + 1}`;
    f.imports.set(token, b);
    const preview: AppImportPreview = {
      token,
      manifest: b.app.manifest,
      files: b.files.length,
      bytes: b.files.reduce((n, x) => n + x.content.length, 0),
      replaces: f.apps.has(b.app.manifest.id) && !f.apps.get(b.app.manifest.id)?.app.builtin,
      renamedFrom: null,
    };
    return ok(preview);
  },
  installAppImport: (token) => {
    const b = f.imports.get(token);
    if (!b) return fail("The import expired. Choose the file again.");
    f.imports.delete(token);
    f.apps.set(b.app.manifest.id, b);
    f.prefs.delete(b.app.manifest.id);
    return ok(b.app);
  },
  appPrefs: (id) =>
    ok({
      prefs: prefsOf(id),
      textModel: prefsOf(id).textModel ?? "lmstudio/fake-model",
      keys: [...(f.keys.get(id) ?? [])],
      storageBytes: f.storage.get(id) ?? 0,
    }),
  saveAppPrefs: (id, prefs) => {
    const old = prefsOf(id);
    const saved = { ...prefs, granted: prefs.granted.filter((c) => old.granted.includes(c)) };
    f.prefs.set(id, saved);
    return ok(saved);
  },
  grantAppCapability: (id, capability) => {
    const p = prefsOf(id);
    if (!p.granted.includes(capability)) p.granted = [...p.granted, capability];
    f.prefs.set(id, p);
    return ok(p);
  },
  setAppKey: (id, kind, key) => {
    const set = f.keys.get(id) ?? new Set<GenerationKind>();
    if (key) set.add(kind);
    else set.delete(kind);
    f.keys.set(id, set);
    return ok(null);
  },
  clearAppStorage: (id) => {
    f.storage.set(id, 0);
    return ok(null);
  },
  appCall: (appId, _callId, method, paramsJson) => {
    const params: unknown = JSON.parse(paramsJson);
    const b = f.apps.get(appId);
    const err = (e: AppCallError): Promise<Result<string, AppCallError>> =>
      Promise.resolve({ status: "error", error: e });
    if (!b) return err({ code: "failed", message: "app not found", capability: null });
    const cap = NEEDS[method];
    if (cap) {
      if (!(b.app.manifest.capabilities ?? []).includes(cap)) {
        return err({ code: "undeclared", message: `undeclared ${cap}`, capability: cap });
      }
      if (!prefsOf(appId).granted.includes(cap)) {
        return err({
          code: "needsPermission",
          message: `${b.app.manifest.name} wants to ${cap}.`,
          capability: cap,
        });
      }
    }
    f.calls.push({ appId, method, params });
    try {
      return Promise.resolve({
        status: "ok",
        data: JSON.stringify(f.answer(appId, method, params) ?? null),
      });
    } catch (e) {
      return err({
        code: "failed",
        message: e instanceof Error ? e.message : String(e),
        capability: null,
      });
    }
  },
  cancelAppCall: () => ok(null),
  saveAppOutput: (name) => ok(`/Users/test/Downloads/${name}`),
};
