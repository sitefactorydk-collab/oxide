// The signing key for in-app updates (tauri-plugin-updater).
//
//   node scripts/updater-key.mjs              makes the key pair if it doesn't exist yet and
//                                             writes the public key into tauri.conf.json
//   node scripts/updater-key.mjs --show-path  prints where the private key is
//
// The private key stays on this computer (~/.tauri/oxide-updater.key, no password) and goes
// into the GitHub secret TAURI_SIGNING_PRIVATE_KEY (docs/RELEASING.md). Losing it means
// installed copies can't verify later updates, so keep a backup. The public key is committed:
// `plugins.updater.pubkey` in src-tauri/tauri.conf.json.

import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const key = process.env.OXIDE_UPDATER_KEY ?? join(homedir(), ".tauri", "oxide-updater.key");
const conf = join(root, "src-tauri", "tauri.conf.json");

if (process.argv.includes("--show-path")) {
  console.log(key);
  process.exit(0);
}

if (!existsSync(key)) {
  mkdirSync(dirname(key), { recursive: true });
  const cli = join(root, "node_modules", "@tauri-apps", "cli", "tauri.js");
  console.log(`Making the update signing key: ${key}`);
  execFileSync(process.execPath, [cli, "signer", "generate", "--ci", "-w", key], {
    stdio: "inherit",
  });
}
const pub = readFileSync(`${key}.pub`, "utf8").trim();
if (!pub) throw new Error(`${key}.pub is empty`);

const config = JSON.parse(readFileSync(conf, "utf8"));
config.plugins ??= {};
config.plugins.updater ??= {};
if (config.plugins.updater.pubkey === pub) {
  console.log("tauri.conf.json already has this public key.");
} else {
  config.plugins.updater.pubkey = pub;
  writeFileSync(conf, `${JSON.stringify(config, null, 2)}\n`);
  console.log("Wrote the public key into src-tauri/tauri.conf.json (commit it).");
}
