// Builds the Chrome extension into extension/dist: TypeScript (tsc, no bundler: the service
// worker and popup are ES modules, the content scripts plain scripts) plus the static files.
// Load extension/dist with "Load unpacked" in chrome://extensions.

import { execFileSync } from "node:child_process";
import { cpSync, mkdirSync, rmSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const ext = join(root, "extension");
const dist = join(ext, "dist");
const require = createRequire(import.meta.url);
const tsc = require.resolve("typescript/bin/tsc");

rmSync(dist, { recursive: true, force: true });
mkdirSync(dist, { recursive: true });
execFileSync(process.execPath, [tsc, "-p", join(ext, "tsconfig.json")], { stdio: "inherit" });
rmSync(join(dist, "chrome.d.ts"), { force: true });
cpSync(join(ext, "static"), dist, { recursive: true });
console.log(`Chrome extension built: ${dist}`);
