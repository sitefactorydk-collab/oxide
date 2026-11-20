import * as Babel from "@babel/standalone";
import { describe, expect, it } from "vitest";
import type { Transform } from "@/frame/run";
import { AppLoadError, appComponent, joinPath, loadApp, resolveFile } from "./loader";

const transform = ((Babel as { transform?: Transform }).transform ??
  (Babel as unknown as { default: { transform: Transform } }).default.transform) as Transform;

describe("loading a Utility App's files", () => {
  it("joins and resolves relative imports inside the app", () => {
    expect(joinPath("App.tsx", "./lib/a")).toBe("lib/a");
    expect(joinPath("lib/a.ts", "../b")).toBe("b");
    expect(() => joinPath("App.tsx", "../../x")).toThrow(AppLoadError);
    const files = new Map([
      ["lib/a.ts", ""],
      ["ui/index.tsx", ""],
      ["data.json", ""],
    ]);
    expect(resolveFile(files, "lib/a")).toBe("lib/a.ts");
    expect(resolveFile(files, "ui")).toBe("ui/index.tsx");
    expect(resolveFile(files, "data.json")).toBe("data.json");
    expect(resolveFile(files, "nope")).toBeUndefined();
  });

  it("compiles several files with TypeScript, JSON, CSS and bundled modules", () => {
    const styles: string[] = [];
    const react = { createElement: (...a: unknown[]) => a };
    const exports = loadApp(
      [
        {
          path: "App.tsx",
          content: `import { double } from "./lib/math";
import data from "./data.json";
import "./app.css";
import { ai } from "oxide";
export default function App(): number { return double(data.n) + ai.base; }`,
        },
        { path: "lib/math.ts", content: "export const double = (n: number): number => n * 2;" },
        { path: "data.json", content: '{ "n": 20 }' },
        { path: "app.css", content: ".x { color: red }" },
      ],
      "App.tsx",
      {
        transform,
        modules: { react, oxide: { __esModule: true, ai: { base: 2 } } },
        addStyle: (css) => styles.push(css),
      },
    );
    const App = appComponent(exports) as () => number;
    expect(App()).toBe(42);
    expect(styles).toEqual([".x { color: red }"]);
  });

  it("says what is wrong: unknown modules, missing files, syntax errors, no component", () => {
    const opts = { transform, modules: { react: {} } };
    expect(() =>
      loadApp(
        [{ path: "App.tsx", content: 'import x from "lodash"; export default x;' }],
        "App.tsx",
        opts,
      ),
    ).toThrow(/“lodash” can't be imported here. Apps can use: react/);
    expect(() =>
      loadApp([{ path: "App.tsx", content: 'import "./missing";' }], "App.tsx", opts),
    ).toThrow(/isn't a file of this app/);
    expect(() => loadApp([{ path: "App.tsx", content: "let = ;" }], "App.tsx", opts)).toThrow(
      AppLoadError,
    );
    expect(() => loadApp([], "App.tsx", opts)).toThrow(/entry file App.tsx is missing/);
    expect(() => appComponent({ x: 1 })).toThrow(/no component/);
  });
});
