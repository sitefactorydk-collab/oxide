import { describe, expect, it } from "vitest";
import { ArtifactError, BABEL_OPTIONS, compile, evaluate, pickComponent } from "./run";

const react = { createElement: () => null };

describe("running React artifacts", () => {
  it("evaluates CommonJS output with the bundled modules only", () => {
    const exports = evaluate(
      'const r = require("react"); exports.default = function App() { return r; };',
      { react },
    );
    expect(typeof exports.default).toBe("function");
    expect((exports.default as () => unknown)()).toBe(react);
    expect(() => evaluate('require("lodash")', { react, "lucide-react": {} })).toThrow(
      /“lodash” can’t be imported here. Artifacts can use: react, lucide-react/,
    );
  });

  it("finds the component: default export, first function, or an unexported App", () => {
    const f = () => null;
    expect(pickComponent({ default: f })).toBe(f);
    expect(pickComponent({ helper: 1, Thing: f })).toBe(f);
    expect(() => pickComponent({})).toThrow(ArtifactError);
    const exports = evaluate("function App() { return 1; }", { react });
    expect(typeof exports.default).toBe("function");
  });

  it("passes TypeScript, JSX and module options to Babel, and reports its errors", () => {
    let seen: Record<string, unknown> = {};
    const App = compile(
      "source",
      (code, options) => {
        seen = options;
        expect(code).toBe("source");
        return { code: "exports.default = () => 'hi';" };
      },
      { react },
    );
    expect((App as () => string)()).toBe("hi");
    expect(seen).toBe(BABEL_OPTIONS);
    expect(JSON.stringify(seen)).toContain("transform-modules-commonjs");
    expect(() =>
      compile(
        "<div",
        () => {
          throw new SyntaxError("Unexpected token (1:4)");
        },
        { react },
      ),
    ).toThrow("Unexpected token (1:4)");
  });
});
