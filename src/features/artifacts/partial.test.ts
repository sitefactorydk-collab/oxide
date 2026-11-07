import { describe, expect, it } from "vitest";
import { artifactCallInfo, artifactRefOf, parseKind, partialFields } from "./partial";

describe("partialFields", () => {
  it("reads complete objects and skips values that aren't strings", () => {
    expect(partialFields('{"a":"x","n":3,"o":{"b":"no","c":[1,"}"]},"t":true,"z":"last"}')).toEqual(
      { a: "x", z: "last" },
    );
  });

  it("returns a string cut off mid-way as far as it has arrived", () => {
    expect(partialFields('{"title":"Notes","content":"# Hi\\nline')).toEqual({
      title: "Notes",
      content: "# Hi\nline",
    });
    // A key that hasn't finished arriving isn't there yet.
    expect(partialFields('{"title":"Notes","cont')).toEqual({ title: "Notes" });
    expect(partialFields('{"title":')).toEqual({});
    expect(partialFields("")).toEqual({});
    expect(partialFields("[1]")).toEqual({});
  });

  it("unescapes JSON escapes, dropping a half-arrived one", () => {
    expect(partialFields('{"c":"a\\"b\\\\c\\t\\u00e6"}').c).toBe('a"b\\c\tæ');
    expect(partialFields('{"c":"tail\\').c).toBe("tail");
    expect(partialFields('{"c":"tail\\u00').c).toBe("tail");
    // An escaped quote doesn't end the string.
    expect(partialFields('{"c":"say \\"title\\": x","title":"T"}')).toEqual({
      c: 'say "title": x',
      title: "T",
    });
  });
});

describe("artifact calls", () => {
  it("say what they make while they stream", () => {
    const info = artifactCallInfo(
      '{"command":"create","id":"app","type":"react","title":"  To-do\\n app ","content":"export',
    );
    expect(info).toEqual({
      command: "create",
      key: "app",
      kind: "react",
      title: "To-do app",
      language: null,
      content: "export",
    });
    expect(artifactCallInfo('{"command":"update","id":"app","old_str":"a"}').content).toBeNull();
    expect(artifactCallInfo('{"command":"bogus"}').command).toBeNull();
  });

  it("parse types forgivingly", () => {
    expect(parseKind("text/markdown")).toBe("markdown");
    expect(parseKind("image/svg+xml")).toBe("svg");
    expect(parseKind("JSX")).toBe("react");
    expect(parseKind("sheet")).toBeNull();
    expect(parseKind(undefined)).toBeNull();
  });

  it("find the artifact a result points to", () => {
    expect(
      artifactRefOf([
        { type: "text", text: "Created" },
        { type: "artifactRef", artifactId: "a1", title: "T", version: 2 },
      ]),
    ).toEqual({ artifactId: "a1", title: "T", version: 2 });
    expect(artifactRefOf([{ type: "text", text: "x" }])).toBeNull();
    expect(artifactRefOf(null)).toBeNull();
  });
});
