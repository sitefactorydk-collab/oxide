import { readFileSync } from "node:fs";
import path from "node:path";
import { expect, it } from "vitest";
import { APP_ICONS } from "./appIcons";

// The builder tells its model which icon names the gallery draws.
it("the builder's prompt lists the gallery's icons", () => {
  const rust = readFileSync(
    path.join(process.cwd(), "crates/oxide-agent/src/apps/builder.rs"),
    "utf8",
  );
  const block = rust.split("pub const APP_ICON_NAMES")[1]?.split("];")[0] ?? "";
  const names = [...block.matchAll(/"([a-z-]+)"/g)].map((m) => m[1]).sort();
  expect(names).toEqual(Object.keys(APP_ICONS).sort());
});
