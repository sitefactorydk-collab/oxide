import type {
  AppBundle,
  AppCheckpoint,
  CheckpointChange,
  AppDraft,
  PreviewReport,
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
  /** Builder drafts by app id (Phase 21). */
  drafts: Map<string, AppBundle>;
  /** Builder chats by app id. */
  builderChats: Map<string, string>;
  /** Checkpoints by app id, newest first, with their files. */
  checkpoints: Map<string, Array<AppCheckpoint & { snapshot: AppFile[] }>>;
  /** The Preview's answers to the builder model (`answerPreviewRequest`). */
  previewAnswers: Array<{ requestId: string; report: PreviewReport }>;
  /** What the next folder pick (`chooseAppFolder`) returns (`null`: cancelled). */
  nextFolder: string | null;
}

/** How the fake builder reaches the fake backend's chats and events. */
interface BuilderHooks {
  createChat: (title: string, model: string) => string;
  deleteChat: (id: string) => void;
  draftChanged: (appId: string) => void;
}
let hooks: BuilderHooks | null = null;
export function setBuilderHooks(h: BuilderHooks): void {
  hooks = h;
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
  "folder.list": "folder",
  "folder.read": "folder",
  "folder.write": "folder",
  "folder.mkdir": "folder",
  "folder.delete": "folder",
  "folder.rename": "folder",
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
    drafts: new Map(),
    builderChats: new Map(),
    checkpoints: new Map(),
    previewAnswers: [],
    nextFolder: null,
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
  folder: null,
});
const prefsOf = (id: string) => f.prefs.get(id) ?? emptyPrefs();

function list(): UtilityApp[] {
  const all = [...f.apps.values()].map((b) => b.app);
  return [
    ...all.filter((a) => a.builtin),
    ...all.filter((a) => !a.builtin).sort((a, b) => a.manifest.name.localeCompare(b.manifest.name)),
  ];
}

// ---- the builder (Phase 21) ----

const TEMPLATE_APP = "export default function App() {\n  return <p>New app</p>;\n}\n";
let checkpointSeq = 0;

function draftOf(id: string): AppDraft | null {
  const d = f.drafts.get(id);
  const chat = f.builderChats.get(id);
  if (!d || !chat) return null;
  const saved = f.apps.get(id);
  const same =
    saved !== undefined &&
    JSON.stringify([...saved.files].sort((a, b) => a.path.localeCompare(b.path))) ===
      JSON.stringify([...d.files].sort((a, b) => a.path.localeCompare(b.path)));
  return {
    app: { ...d.app, folder: `/data/app-drafts/${id}` },
    files: d.files,
    published: saved !== undefined,
    unsaved: !same,
    conversationId: chat,
  };
}

/** Like the backend's: what changed since the previous snapshot, in (rough) lines. */
function changesBetween(before: AppFile[] | undefined, after: AppFile[]): CheckpointChange[] {
  if (!before) return [];
  const lines = (t: string) => (t === "" ? [] : t.replace(/\n$/, "").split("\n"));
  const out: CheckpointChange[] = [];
  for (const file of after) {
    const old = before.find((b) => b.path === file.path);
    if (!old) {
      out.push({ path: file.path, kind: "added", added: lines(file.content).length, removed: 0 });
    } else if (old.content !== file.content) {
      const a = lines(old.content);
      const b = lines(file.content);
      out.push({
        path: file.path,
        kind: "changed",
        added: b.filter((l) => !a.includes(l)).length,
        removed: a.filter((l) => !b.includes(l)).length,
      });
    }
  }
  for (const old of before) {
    if (!after.some((a) => a.path === old.path)) {
      out.push({ path: old.path, kind: "deleted", added: 0, removed: lines(old.content).length });
    }
  }
  return out.sort((x, y) => x.path.localeCompare(y.path));
}

function checkpoint(id: string, label: string, kind: AppCheckpoint["kind"]) {
  const d = f.drafts.get(id);
  if (!d) return;
  const list = f.checkpoints.get(id) ?? [];
  const snapshot = d.files.map((x) => ({ ...x }));
  if (list[0] && JSON.stringify(list[0].snapshot) === JSON.stringify(snapshot)) return;
  list.unshift({
    id: `cp${++checkpointSeq}`,
    appId: id,
    label,
    kind,
    files: snapshot.length,
    createdAt: Date.now(),
    changes: changesBetween(list[0]?.snapshot, snapshot),
    snapshot,
  });
  f.checkpoints.set(id, list);
}

/** Writes a draft file as the builder's model would (and tells the window). */
export function modelWritesDraftFile(id: string, path: string, content: string): void {
  const d = f.drafts.get(id);
  if (!d) throw new Error(`no draft ${id}`);
  f.drafts.set(id, {
    ...d,
    files: d.files.filter((x) => x.path !== path).concat({ path, content }),
  });
  hooks?.draftChanged(id);
}

/** The draft's text for a path. */
export function draftFile(id: string, path: string): string | undefined {
  return f.drafts.get(id)?.files.find((x) => x.path === path)?.content;
}

function withManifest(b: AppBundle, manifest: AppManifest): AppBundle {
  return {
    app: { ...b.app, manifest },
    files: b.files
      .filter((x) => x.path !== "oxide-app.json")
      .concat({ path: "oxide-app.json", content: JSON.stringify(manifest) }),
  };
}

const okDraft = (id: string) => {
  const d = draftOf(id);
  return d ? ok(d) : fail("draft not found");
};

export const fakeBuilderCommands: Pick<
  typeof RealCommands,
  | "openAppBuilder"
  | "appDraft"
  | "listAppDrafts"
  | "saveDraftFile"
  | "deleteDraftFile"
  | "renameDraftFile"
  | "updateDraftDetails"
  | "appCheckpoints"
  | "restoreAppCheckpoint"
  | "publishAppDraft"
  | "discardAppDraft"
  | "answerPreviewRequest"
> = {
  openAppBuilder: (id, model) => {
    let appId = id;
    if (appId === null) {
      appId = "new-app";
      for (let n = 2; f.drafts.has(appId) || f.apps.has(appId); n++) appId = `new-app-${n}`;
      const manifest = fakeManifest({ id: appId, name: "New app", capabilities: ["text"] });
      const b = fakeBundle(manifest, false, [{ path: "App.tsx", content: TEMPLATE_APP }]);
      f.drafts.set(appId, b);
    } else if (!f.drafts.has(appId)) {
      const saved = f.apps.get(appId);
      if (!saved) return fail("app not found");
      if (saved.app.builtin) return fail("Built-in apps can't be changed. Clone the app first.");
      f.drafts.set(appId, { ...saved, files: saved.files.map((x) => ({ ...x })) });
    }
    if (!f.builderChats.has(appId)) {
      const name = f.drafts.get(appId)?.app.manifest.name ?? appId;
      f.builderChats.set(appId, hooks?.createChat(`Building ${name}`, model) ?? "b-0");
    }
    checkpoint(appId, "Start", "start");
    return okDraft(appId);
  },
  appDraft: (id) => okDraft(id),
  listAppDrafts: () =>
    ok([...f.drafts.entries()].filter(([id]) => !f.apps.has(id)).map(([, b]) => b.app)),
  saveDraftFile: (id, path, content) => {
    const d = f.drafts.get(id);
    if (!d) return fail("draft not found");
    f.drafts.set(id, {
      ...d,
      files: d.files.filter((x) => x.path !== path).concat({ path, content }),
    });
    checkpoint(id, `Saved ${path}`, "manual");
    return okDraft(id);
  },
  deleteDraftFile: (id, path) => {
    const d = f.drafts.get(id);
    if (!d) return fail("draft not found");
    f.drafts.set(id, { ...d, files: d.files.filter((x) => x.path !== path) });
    checkpoint(id, `Deleted ${path}`, "manual");
    return okDraft(id);
  },
  renameDraftFile: (id, from, to) => {
    const d = f.drafts.get(id);
    if (!d) return fail("draft not found");
    f.drafts.set(id, {
      ...d,
      files: d.files.map((x) => (x.path === from ? { ...x, path: to } : x)),
    });
    checkpoint(id, `Renamed ${from}`, "manual");
    return okDraft(id);
  },
  updateDraftDetails: (id, name, description, icon, category) => {
    const d = f.drafts.get(id);
    if (!d) return fail("draft not found");
    f.drafts.set(id, withManifest(d, { ...d.app.manifest, name, description, icon, category }));
    checkpoint(id, "Changed the app's details", "manual");
    return okDraft(id);
  },
  appCheckpoints: (id) =>
    ok(
      (f.checkpoints.get(id) ?? []).map((c): AppCheckpoint => ({
        id: c.id,
        appId: c.appId,
        label: c.label,
        kind: c.kind,
        files: c.files,
        createdAt: c.createdAt,
        changes: c.changes,
      })),
    ),
  restoreAppCheckpoint: (id, checkpointId) => {
    const d = f.drafts.get(id);
    const cp = (f.checkpoints.get(id) ?? []).find((c) => c.id === checkpointId);
    if (!d || !cp) return fail("checkpoint not found");
    checkpoint(id, "Before restoring", "restore");
    f.drafts.set(id, { ...d, files: cp.snapshot.map((x) => ({ ...x })) });
    return okDraft(id);
  },
  publishAppDraft: (id) => {
    const d = f.drafts.get(id);
    if (!d) return fail("draft not found");
    const app = { ...d.app, builtin: false, folder: `/data/apps/${id}` };
    f.apps.set(id, { app, files: d.files.map((x) => ({ ...x })) });
    return ok(app);
  },
  discardAppDraft: (id) => {
    f.drafts.delete(id);
    if (!f.apps.has(id)) {
      const chat = f.builderChats.get(id);
      if (chat) hooks?.deleteChat(chat);
      f.builderChats.delete(id);
      f.checkpoints.delete(id);
    }
    return ok(null);
  },
  answerPreviewRequest: (requestId, report) => {
    f.previewAnswers.push({ requestId, report });
    return ok(true);
  },
};

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
  | "readDroppedFiles"
  | "chooseAppFolder"
  | "clearAppFolder"
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
    const saved = {
      ...prefs,
      granted: prefs.granted.filter((c) => old.granted.includes(c)),
      folder: old.folder,
    };
    f.prefs.set(id, saved);
    return ok(saved);
  },
  grantAppCapability: (ref, capability) => {
    const id = ref.replace(/^draft:/, "");
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
  appCall: (ref, _callId, method, paramsJson) => {
    const params: unknown = JSON.parse(paramsJson);
    // The builder's Preview runs the draft with the app's grants.
    const appId = ref.replace(/^draft:/, "");
    const b = ref.startsWith("draft:") ? f.drafts.get(appId) : f.apps.get(appId);
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
  chooseAppFolder: (ref) => {
    const id = ref.replace(/^draft:/, "");
    const folder = f.nextFolder;
    if (folder === null) return ok(null);
    const p = prefsOf(id);
    f.prefs.set(id, {
      ...p,
      folder,
      granted: p.granted.includes("folder") ? p.granted : [...p.granted, "folder"],
    });
    return ok(folder);
  },
  clearAppFolder: (ref) => {
    const id = ref.replace(/^draft:/, "");
    const p = prefsOf(id);
    f.prefs.set(id, { ...p, folder: null, granted: p.granted.filter((c) => c !== "folder") });
    return ok(null);
  },
  // Dropped files read back: "png" as their content.
  readDroppedFiles: (paths) =>
    ok(
      paths.map((p) => ({
        name: p.split(/[\\/]/).pop() ?? p,
        mediaType: p.endsWith(".zip") ? "application/zip" : "image/png",
        dataBase64: "cG5n",
      })),
    ),
};
