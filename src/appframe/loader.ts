/**
 * Runs a Utility App's source files: each TypeScript/JSX file is compiled by Babel to
 * CommonJS on first `require`, relative imports resolve between the app's files, CSS files
 * become `<style>` tags, JSON files are parsed, and bare imports come from the bundled
 * libraries (the frame's module map). Pure apart from evaluation, so it can be tested
 * without a frame.
 */
import { BABEL_OPTIONS, type ModuleMap, type Transform } from "@/frame/run";
import type { FrameFile } from "./protocol";

export class AppLoadError extends Error {}

const EXTENSIONS = [".tsx", ".ts", ".jsx", ".js", ".json", ".css"];

/** Joins a relative import onto the importing file's folder (`/`-separated, no leading `/`). */
export function joinPath(from: string, spec: string): string {
  const parts = from.split("/").slice(0, -1);
  for (const seg of spec.split("/")) {
    if (seg === "" || seg === ".") continue;
    if (seg === "..") {
      if (parts.length === 0) throw new AppLoadError(`“${spec}” points outside the app.`);
      parts.pop();
    } else parts.push(seg);
  }
  return parts.join("/");
}

/** The app file an import names: as is, with an extension, or a folder's index file. */
export function resolveFile(files: Map<string, string>, path: string): string | undefined {
  if (files.has(path)) return path;
  for (const ext of EXTENSIONS) if (files.has(path + ext)) return path + ext;
  for (const ext of EXTENSIONS) if (files.has(`${path}/index${ext}`)) return `${path}/index${ext}`;
  return undefined;
}

export interface LoadOptions {
  transform: Transform;
  modules: ModuleMap;
  /** Adds a stylesheet (CSS files the app imports). */
  addStyle?: (css: string, path: string) => void;
}

/** Loads the app and returns the entry file's exports. */
export function loadApp(
  files: FrameFile[],
  entry: string,
  { transform, modules, addStyle }: LoadOptions,
): Record<string, unknown> {
  const sources = new Map(files.map((f) => [f.path, f.content]));
  const cache = new Map<string, { exports: Record<string, unknown> }>();
  const known = Object.keys(modules)
    .filter((m) => !m.includes("/") || m.startsWith("@"))
    .join(", ");

  const load = (path: string): Record<string, unknown> => {
    const hit = cache.get(path);
    if (hit) return hit.exports;
    const source = sources.get(path) ?? "";
    const module: { exports: Record<string, unknown> } = { exports: {} };
    cache.set(path, module);
    if (path.endsWith(".css")) {
      addStyle?.(source, path);
      return module.exports;
    }
    if (path.endsWith(".json")) {
      try {
        module.exports = { default: JSON.parse(source) } as Record<string, unknown>;
        cache.set(path, module);
      } catch (e) {
        throw new AppLoadError(`${path} isn't valid JSON: ${e instanceof Error ? e.message : e}`);
      }
      return module.exports;
    }
    let compiled: string;
    try {
      compiled = transform(source, { ...BABEL_OPTIONS, filename: path }).code ?? "";
    } catch (e) {
      throw new AppLoadError(e instanceof Error ? e.message : String(e));
    }
    const require = (spec: string): unknown => {
      if (spec.startsWith(".") || spec.startsWith("/")) {
        const target = resolveFile(sources, joinPath(path, spec.replace(/^\/+/, "./")));
        if (!target) throw new AppLoadError(`${path}: “${spec}” isn't a file of this app.`);
        const exp = load(target);
        // JSON modules: `import data from "./x.json"` and `require("./x.json")` both work.
        return target.endsWith(".json") ? { ...exp, __esModule: true } : exp;
      }
      if (Object.prototype.hasOwnProperty.call(modules, spec)) return modules[spec];
      throw new AppLoadError(`${path}: “${spec}” can't be imported here. Apps can use: ${known}.`);
    };
    const fn = new Function("require", "module", "exports", "React", compiled) as (
      r: typeof require,
      m: typeof module,
      e: Record<string, unknown>,
      react: unknown,
    ) => void;
    fn(require, module, module.exports, modules.react);
    return module.exports;
  };

  const start = resolveFile(sources, entry);
  if (!start) throw new AppLoadError(`The entry file ${entry} is missing.`);
  return load(start);
}

/** The component an app renders: its entry's default export, else its first function. */
export function appComponent(exports: Record<string, unknown>): unknown {
  const d = exports.default;
  if (typeof d === "function" || (typeof d === "object" && d !== null)) return d;
  const first = Object.values(exports).find((v) => typeof v === "function");
  if (first) return first;
  throw new AppLoadError(
    "The app's entry file has no component. Export one with `export default function App()`.",
  );
}
