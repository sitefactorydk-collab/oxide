// Sets Oxide's version everywhere it is written by hand:
//   node scripts/set-version.mjs 0.1.1
// package.json, src-tauri/tauri.conf.json and the workspace version in Cargo.toml (every
// crate inherits it). Cargo.lock and package-lock.json follow on the next cargo/npm run.
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const version = process.argv[2];
if (!/^\d+\.\d+\.\d+$/.test(version ?? "")) {
  console.error("Usage: node scripts/set-version.mjs <major.minor.patch>");
  process.exit(1);
}
const root = join(dirname(fileURLToPath(import.meta.url)), "..");
for (const file of ["package.json", "src-tauri/tauri.conf.json"]) {
  const path = join(root, file);
  const json = JSON.parse(readFileSync(path, "utf8"));
  json.version = version;
  writeFileSync(path, `${JSON.stringify(json, null, 2)}\n`);
}
const cargo = join(root, "Cargo.toml");
const text = readFileSync(cargo, "utf8");
const next = text.replace(/(\[workspace\.package\][^[]*?\nversion = )"[^"]*"/, `$1"${version}"`);
if (next === text && !text.includes(`version = "${version}"`)) {
  console.error("Couldn't find the workspace version in Cargo.toml");
  process.exit(1);
}
writeFileSync(cargo, next);
console.log(`Version ${version}`);
