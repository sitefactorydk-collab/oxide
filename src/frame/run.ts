/**
 * Turning a React artifact's source into a component: Babel (TypeScript + JSX → CommonJS),
 * then evaluation with a `require` that knows only the bundled modules. Pure apart from
 * the evaluation, so it can be tested without a frame.
 */

/** Modules an artifact may import. */
export type ModuleMap = Record<string, unknown>;

export type Transform = (
  code: string,
  options: Record<string, unknown>,
) => { code?: string | null };

export const BABEL_OPTIONS: Record<string, unknown> = {
  filename: "artifact.tsx",
  sourceType: "module",
  presets: [
    ["typescript", { isTSX: true, allExtensions: true }],
    ["react", { runtime: "classic" }],
  ],
  plugins: ["transform-modules-commonjs"],
};

export class ArtifactError extends Error {}

/** Evaluates transformed code and returns what the module exports. */
export function evaluate(compiled: string, modules: ModuleMap): Record<string, unknown> {
  const require = (name: string): unknown => {
    if (Object.prototype.hasOwnProperty.call(modules, name)) return modules[name];
    const known = Object.keys(modules)
      .filter((m) => !m.includes("/"))
      .join(", ");
    throw new ArtifactError(`“${name}” can’t be imported here. Artifacts can use: ${known}.`);
  };
  const module: { exports: Record<string, unknown> } = { exports: {} };
  // Components the model defined but didn't export are picked up too.
  const fallback =
    '\n;if (!module.exports.default) { try { if (typeof App !== "undefined") module.exports.default = App; } catch (e) {} }';
  const fn = new Function("require", "module", "exports", "React", compiled + fallback) as (
    r: typeof require,
    m: typeof module,
    e: Record<string, unknown>,
    react: unknown,
  ) => void;
  fn(require, module, module.exports, modules.react);
  return module.exports;
}

/** The component to render: the default export, else the first exported function. */
export function pickComponent(exports: Record<string, unknown>): unknown {
  const d = exports.default;
  if (typeof d === "function" || (typeof d === "object" && d !== null)) return d;
  const first = Object.values(exports).find((v) => typeof v === "function");
  if (first) return first;
  throw new ArtifactError(
    "The artifact has no component to show. Export one with `export default function App()`.",
  );
}

/** Source → component. */
export function compile(source: string, transform: Transform, modules: ModuleMap): unknown {
  let compiled: string;
  try {
    compiled = transform(source, BABEL_OPTIONS).code ?? "";
  } catch (e) {
    throw new ArtifactError(e instanceof Error ? e.message : String(e));
  }
  return pickComponent(evaluate(compiled, modules));
}
