import { readFileSync } from "node:fs";
import { expect, it } from "vitest";
import { DTS_FILE, sdkTypes } from "../../scripts/sdk-dts.mjs";

// The app builder's model learns the SDK from oxide-sdk.d.ts (compiled into Oxide). It must
// say what sdk.ts does: after changing the SDK, run `npm run sdk-types`.
it("oxide-sdk.d.ts matches sdk.ts", async () => {
  const fresh = await sdkTypes();
  expect(readFileSync(DTS_FILE, "utf8").replace(/\r\n/g, "\n")).toBe(fresh);
  expect(fresh).toContain("export declare const ai");
  expect(fresh).not.toContain("function connect");
}, 120_000);
