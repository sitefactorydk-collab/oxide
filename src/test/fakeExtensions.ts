import type {
  CommandError,
  Extension,
  ExtensionInfo,
  ExtensionPreview,
  ExtensionValue,
  PickKind,
  Result,
  Skill,
  commands as RealCommands,
} from "@/bindings";

/**
 * Extensions and skills in the fake backend. `.mcpb` paths open the bundles registered with
 * `fakeBundle` (else a default one); pickers return what `setPickedPaths` set.
 */
type Values = Partial<Record<string, ExtensionValue>>;

interface FakeSkill {
  skill: Skill;
  body: string;
  files: string[];
}

interface FakeExtensions {
  installed: Extension[];
  staged: Map<string, { info: ExtensionInfo }>;
  /** Sensitive values, by `name/key`. */
  secrets: Map<string, string>;
  bundles: Map<string, ExtensionInfo>;
  picked: Partial<Record<PickKind, string[]>>;
  picks: Array<{ kind: PickKind; multiple: boolean }>;
  skills: FakeSkill[];
  opened: Array<string | null>;
  discarded: string[];
}

let ext: FakeExtensions;
let seq = 0;

export function resetFakeExtensions(): void {
  seq = 0;
  ext = {
    installed: [],
    staged: new Map(),
    secrets: new Map(),
    bundles: new Map(),
    picked: {},
    picks: [],
    skills: [],
    opened: [],
    discarded: [],
  };
}
resetFakeExtensions();

export function fakeExtensionsState(): Readonly<FakeExtensions> {
  return ext;
}

/** What the picker of `kind` returns next. */
export function setPickedPaths(kind: PickKind, paths: string[]): void {
  ext.picked[kind] = paths;
}

export function sampleExtensionInfo(overrides: Partial<ExtensionInfo> = {}): ExtensionInfo {
  return {
    name: "weather",
    displayName: "Weather",
    version: "1.0.0",
    description: "Forecasts for any city",
    longDescription: null,
    author: "Ann Example",
    authorUrl: null,
    homepage: "https://example.com/weather",
    repository: null,
    license: "MIT",
    manifestVersion: "0.3",
    serverType: "node",
    tools: [{ name: "forecast", description: "Get a forecast" }],
    prompts: [],
    toolsGenerated: false,
    keywords: [],
    platforms: [],
    runtimes: { node: ">=18" },
    fields: [
      {
        key: "api_key",
        type: "string",
        title: "API key",
        description: "From your weather account",
        required: true,
        default: null,
        multiple: false,
        sensitive: true,
        min: null,
        max: null,
      },
      {
        key: "units",
        type: "string",
        title: "Units",
        description: null,
        required: false,
        default: "metric",
        multiple: false,
        sensitive: false,
        min: null,
        max: null,
      },
      {
        key: "days",
        type: "number",
        title: "Days",
        description: null,
        required: false,
        default: 3,
        multiple: false,
        sensitive: false,
        min: 1,
        max: 7,
      },
      {
        key: "folders",
        type: "directory",
        title: "Folders",
        description: null,
        required: false,
        default: null,
        multiple: true,
        sensitive: false,
        min: null,
        max: null,
      },
    ],
    icon: null,
    ...overrides,
  };
}

/** Makes `path` open as a bundle with this manifest. */
export function fakeBundle(path: string, info: ExtensionInfo): void {
  ext.bundles.set(path, info);
}

const ok = <T>(data: T): Promise<Result<T, CommandError>> =>
  Promise.resolve({ status: "ok", data: JSON.parse(JSON.stringify(data)) as T });
const fail = <T>(kind: CommandError["kind"], message: string): Promise<Result<T, CommandError>> =>
  Promise.resolve({ status: "error", error: { kind, message } as CommandError });

function filled(v: ExtensionValue | undefined): boolean {
  if (v === undefined) return false;
  if (typeof v === "string") return v.trim() !== "";
  if (Array.isArray(v)) return v.length > 0;
  return true;
}

/** Like the backend: sensitive values to the "keychain", the rest kept; required checked. */
function apply(e: Extension, info: ExtensionInfo, values: Values): Extension | string {
  for (const f of info.fields) {
    const v = values[f.key];
    if (f.type === "number" && typeof v === "number") {
      if (f.max !== null && v > f.max) return `“${f.title}” must be at most ${f.max}`;
      if (f.min !== null && v < f.min) return `“${f.title}” must be at least ${f.min}`;
    }
  }
  const plain: Values = {};
  const secrets = new Set(e.secretsSet);
  for (const f of info.fields) {
    const v = values[f.key];
    if (!filled(v)) continue;
    if (f.sensitive) {
      ext.secrets.set(`${e.name}/${f.key}`, String(v));
      secrets.add(f.key);
    } else {
      plain[f.key] = v;
    }
  }
  const missing = info.fields.find(
    (f) =>
      f.required &&
      !(f.sensitive ? secrets.has(f.key) : filled(plain[f.key]) || filled(f.default ?? undefined)),
  );
  return {
    ...e,
    info,
    version: info.version,
    values: plain,
    secretsSet: [...secrets],
    status: missing ? "needsSetup" : "ready",
    problem: missing ? `Fill in “${missing.title}” in its settings.` : null,
  };
}

function skillOf(folder: string, name: string, description: string, files: string[]): Skill {
  return {
    name,
    description,
    folder,
    path: `/data/skills/${folder}`,
    enabled: true,
    license: null,
    files: files.length,
    problem: null,
    updatedAt: 1,
  };
}

export function fakeSkill(
  name: string,
  description: string,
  body = "Do it.",
  files: string[] = [],
) {
  const folder = name.toLowerCase().replace(/[^a-z0-9]+/g, "-");
  ext.skills.push({ skill: skillOf(folder, name, description, files), body, files });
}

type Cmds = typeof RealCommands;
export const fakeExtensionCommands: Pick<
  Cmds,
  | "pickPaths"
  | "listExtensions"
  | "previewExtension"
  | "installExtension"
  | "discardExtension"
  | "configureExtension"
  | "setExtensionEnabled"
  | "uninstallExtension"
  | "openExtensionsFolder"
  | "listSkills"
  | "getSkill"
  | "createSkill"
  | "updateSkill"
  | "addSkill"
  | "setSkillEnabled"
  | "removeSkill"
  | "openSkillsFolder"
> = {
  pickPaths: (kind, multiple) => {
    ext.picks.push({ kind, multiple });
    const p = ext.picked[kind] ?? [];
    ext.picked[kind] = [];
    return ok(p);
  },
  listExtensions: () => ok(ext.installed),
  previewExtension: (path) => {
    if (!/\.(mcpb|dxt)$/i.test(path)) return fail("invalid", "This isn't a zip file.");
    const info = ext.bundles.get(path) ?? sampleExtensionInfo();
    seq += 1;
    const stagingId = `stage-${seq}`;
    ext.staged.set(stagingId, { info });
    const existing = ext.installed.find((e) => e.name === info.name);
    const values: Values = {};
    for (const f of info.fields) {
      if (!f.sensitive && f.default !== null) values[f.key] = f.default;
    }
    Object.assign(values, existing?.values ?? {});
    const preview: ExtensionPreview = {
      stagingId,
      info,
      installedVersion: existing?.version ?? null,
      warnings:
        info.platforms.length && !info.platforms.includes("win32")
          ? ["It's made for macOS, so it won't run on this computer."]
          : [],
      values,
      secretsSet: existing?.secretsSet ?? [],
      size: 2048,
      files: 3,
    };
    return ok(preview);
  },
  installExtension: (stagingId, values) => {
    const staged = ext.staged.get(stagingId);
    if (!staged) return fail("invalid", "Open the extension file again (it was closed)");
    const { info } = staged;
    const existing = ext.installed.find((e) => e.name === info.name);
    const base: Extension = existing ?? {
      info,
      name: info.name,
      version: info.version,
      enabled: true,
      status: "ready",
      problem: null,
      dir: `/data/extensions/${info.name}`,
      values: {},
      secretsSet: [],
      installedAt: 1,
    };
    const next = apply(base, info, values);
    if (typeof next === "string") return fail("invalid", next);
    ext.staged.delete(stagingId);
    ext.installed = [...ext.installed.filter((e) => e.name !== info.name), next];
    return ok(next);
  },
  discardExtension: (stagingId) => {
    ext.discarded.push(stagingId);
    ext.staged.delete(stagingId);
    return ok(null);
  },
  configureExtension: (name, values) => {
    const e = ext.installed.find((x) => x.name === name);
    if (!e?.info) return fail("invalid", `There's no extension called "${name}"`);
    const next = apply(e, e.info, values);
    if (typeof next === "string") return fail("invalid", next);
    ext.installed = ext.installed.map((x) => (x.name === name ? next : x));
    return ok(next);
  },
  setExtensionEnabled: (name, enabled) => {
    const e = ext.installed.find((x) => x.name === name);
    if (!e) return fail("invalid", `There's no extension called "${name}"`);
    e.enabled = enabled;
    return ok(null);
  },
  uninstallExtension: (name) => {
    ext.installed = ext.installed.filter((e) => e.name !== name);
    for (const k of [...ext.secrets.keys()]) if (k.startsWith(`${name}/`)) ext.secrets.delete(k);
    return ok(null);
  },
  openExtensionsFolder: (name) => {
    ext.opened.push(name);
    return ok(null);
  },
  listSkills: () => ok(ext.skills.map((s) => s.skill)),
  getSkill: (folder) => {
    const s = ext.skills.find((x) => x.skill.folder === folder);
    if (!s) return fail("notFound", "skill not found");
    return ok({ skill: s.skill, body: s.body, fileList: s.files });
  },
  createSkill: (input) => {
    const folder = input.name
      .trim()
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-|-$/g, "");
    if (!folder) return fail("invalid", "Give the skill a name (letters or digits)");
    if (ext.skills.some((s) => s.skill.folder === folder)) {
      return fail("invalid", `There's already a skill called "${folder}"`);
    }
    const skill = skillOf(folder, folder, input.description.trim(), []);
    ext.skills.push({ skill, body: input.body, files: [] });
    return ok(skill);
  },
  updateSkill: (folder, input) => {
    const s = ext.skills.find((x) => x.skill.folder === folder);
    if (!s) return fail("notFound", "skill not found");
    s.skill = { ...s.skill, name: input.name.trim(), description: input.description.trim() };
    s.body = input.body;
    return ok(s.skill);
  },
  addSkill: (path) => {
    const base = path.split(/[\\/]/).pop() ?? "skill";
    const name = base.replace(/\.(zip|skill)$/i, "").toLowerCase();
    if (ext.skills.some((s) => s.skill.folder === name)) {
      return fail("invalid", `There's already a skill called "${name}"; remove it first`);
    }
    const skill = skillOf(name, name, `Added from ${base}`, ["reference.md"]);
    ext.skills.push({ skill, body: "From disk.", files: ["reference.md"] });
    return ok(skill);
  },
  setSkillEnabled: (folder, enabled) => {
    const s = ext.skills.find((x) => x.skill.folder === folder);
    if (!s) return fail("notFound", "skill not found");
    s.skill = { ...s.skill, enabled };
    return ok(null);
  },
  removeSkill: (folder) => {
    ext.skills = ext.skills.filter((s) => s.skill.folder !== folder);
    return ok(null);
  },
  openSkillsFolder: (folder) => {
    ext.opened.push(folder);
    return ok(null);
  },
};
