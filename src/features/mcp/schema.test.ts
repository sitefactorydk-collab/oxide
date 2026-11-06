import { describe, expect, it } from "vitest";
import { argumentSkeleton } from "./schema";

const skeleton = (schema: unknown) =>
  JSON.parse(argumentSkeleton(JSON.stringify(schema))) as unknown;

describe("argumentSkeleton", () => {
  it("fills each property with an empty value of its type", () => {
    expect(
      skeleton({
        type: "object",
        properties: {
          q: { type: "string" },
          n: { type: "integer" },
          on: { type: "boolean" },
          tags: { type: "array" },
          mode: { type: "string", enum: ["fast", "slow"] },
          limit: { type: "number", default: 10 },
        },
      }),
    ).toEqual({ q: "", n: 0, on: false, tags: [], mode: "fast", limit: 10 });
  });

  it("uses the first choice of anyOf / oneOf and skips null", () => {
    expect(
      skeleton({
        type: "object",
        properties: {
          repoName: { anyOf: [{ type: "string" }, { type: "array", items: { type: "string" } }] },
          cursor: { anyOf: [{ type: "null" }, { type: "string" }] },
          maybe: { type: ["null", "integer"] },
          kind: { oneOf: [{ type: "boolean" }, { type: "string" }] },
        },
      }),
    ).toEqual({ repoName: "", cursor: "", maybe: 0, kind: false });
  });
});
