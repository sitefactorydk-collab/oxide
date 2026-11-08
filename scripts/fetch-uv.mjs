// Fetches the pinned `uv` release for a target into src-tauri/binaries/uv-<triple>[.exe],
// where Tauri's `externalBin` expects the sidecar.
//
//   node scripts/fetch-uv.mjs                     the host's target
//   node scripts/fetch-uv.mjs --target <triple>   another target
//   node scripts/fetch-uv.mjs --all               every target in uv-pins.json
//   node scripts/fetch-uv.mjs --force             download again even if the file exists
//   node scripts/fetch-uv.mjs --archive <file>    use an archive you downloaded yourself
//
// Without --archive, an archive with the right name in your Downloads folder is used first.
// When Node's download fails on Windows, PowerShell's Invoke-WebRequest (which uses the same
// proxy settings as the browser) is tried. Either way the archive must match the pin.
//
// scripts/uv-pins.json holds the release and, per target, the SHA-256 of the archive. A pin
// that is `null` is filled in the first time the archive is downloaded (and the file is
// rewritten, so commit it). From then on, an archive with another hash is refused. When
// GitHub also serves the release's own `.sha256` file, the download must match it too.

import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import {
  chmodSync,
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { homedir, tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const pinsPath = join(here, "uv-pins.json");
const outDir = join(here, "..", "src-tauri", "binaries");

function hostTriple() {
  try {
    const out = execFileSync("rustc", ["-vV"], { encoding: "utf8" });
    const m = /^host: (.+)$/m.exec(out);
    if (m) return m[1].trim();
  } catch {
    // no rustc: guess from Node
  }
  const arch = process.arch === "arm64" ? "aarch64" : "x86_64";
  if (process.platform === "win32") return `${arch}-pc-windows-msvc`;
  if (process.platform === "darwin") return `${arch}-apple-darwin`;
  return `${arch}-unknown-linux-gnu`;
}

function parseArgs(argv) {
  const args = { targets: [], all: false, force: false, archive: null };
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === "--target") args.targets.push(argv[++i]);
    else if (argv[i] === "--all") args.all = true;
    else if (argv[i] === "--force") args.force = true;
    else if (argv[i] === "--archive") args.archive = argv[++i];
    else throw new Error(`Unknown argument: ${argv[i]}`);
  }
  return args;
}

function sha256(buf) {
  return createHash("sha256").update(buf).digest("hex");
}

/** Downloads `url`, retrying network errors and 5xx answers (GitHub sometimes answers 502/504). */
async function download(url, attempts = 4) {
  for (let i = 1; ; i++) {
    let error;
    try {
      const res = await fetch(url, { redirect: "follow" });
      if (res.ok) return Buffer.from(await res.arrayBuffer());
      error = new Error(`${url} answered ${res.status}`);
      if (res.status < 500 || i >= attempts) throw error;
    } catch (e) {
      error = e;
      if (i >= attempts || /answered [1-4]\d\d$/.test(String(e?.message))) throw e;
    }
    const wait = 2000 * i;
    console.log(
      `${error.message ?? error}; trying again in ${wait / 1000} s (${i}/${attempts - 1})`,
    );
    await new Promise((r) => setTimeout(r, wait));
  }
}

/** The release's own checksum for the archive, or null when it can't be read. */
async function publishedHash(url) {
  try {
    const text = (await download(`${url}.sha256`, 1)).toString("utf8");
    return /^[0-9a-f]{64}/i.exec(text.trim())?.[0].toLowerCase() ?? null;
  } catch {
    return null;
  }
}

const PROXY_VARS = [
  "HTTPS_PROXY",
  "https_proxy",
  "HTTP_PROXY",
  "http_proxy",
  "ALL_PROXY",
  "NO_PROXY",
  "NODE_USE_ENV_PROXY",
];

/** The archive: a file given or in Downloads, else Node's download, else PowerShell's. */
async function getArchive(url, name, given) {
  const candidates = given ? [given] : [join(homedir(), "Downloads", name)];
  for (const file of candidates) {
    if (existsSync(file)) {
      console.log(`Using ${file}`);
      return readFileSync(file);
    }
    if (given) throw new Error(`${given} doesn't exist.`);
  }
  console.log(`Downloading ${url}`);
  try {
    return await download(url);
  } catch (e) {
    const set = PROXY_VARS.filter((v) => process.env[v]).map((v) => `${v}=${process.env[v]}`);
    console.log(`Node's download failed: ${e.message ?? e}`);
    console.log(
      set.length ? `Proxy settings Node saw: ${set.join(", ")}` : "Node saw no proxy variables.",
    );
    if (process.platform !== "win32") throw e;
  }
  console.log("Trying PowerShell's Invoke-WebRequest (uses the Windows proxy settings)");
  const tmp = mkdtempSync(join(tmpdir(), "oxide-uv-dl-"));
  const out = join(tmp, name);
  try {
    execFileSync(
      "powershell",
      [
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        `$ProgressPreference='SilentlyContinue'; Invoke-WebRequest -UseBasicParsing -Uri '${url}' -OutFile '${out}'`,
      ],
      { stdio: "inherit" },
    );
    return readFileSync(out);
  } catch {
    throw new Error(
      `Couldn't download ${name}. Download ${url} in your browser (it goes to Downloads) and run this again, or pass --archive <file>.`,
    );
  } finally {
    rmSync(tmp, { recursive: true, force: true });
  }
}

function findBinary(dir, exe) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const p = join(dir, entry.name);
    if (entry.isDirectory()) {
      const found = findBinary(p, exe);
      if (found) return found;
    } else if (entry.name === exe) {
      return p;
    }
  }
  return null;
}

async function fetchTarget(pins, triple, force, archivePath) {
  const pin = pins.targets[triple];
  if (!pin) throw new Error(`No uv pin for ${triple}. Add it to scripts/uv-pins.json.`);
  const windows = triple.includes("windows");
  const exe = windows ? "uv.exe" : "uv";
  const dest = join(outDir, `uv-${triple}${windows ? ".exe" : ""}`);
  if (!force && existsSync(dest) && statSync(dest).size > 0) {
    console.log(`uv ${pins.version} for ${triple}: already at ${dest}`);
    return false;
  }

  const url = `https://github.com/astral-sh/uv/releases/download/${pins.version}/${pin.archive}`;
  const archive = await getArchive(url, pin.archive, archivePath);
  const hash = sha256(archive);

  if (pin.sha256 && pin.sha256.toLowerCase() !== hash) {
    throw new Error(
      `${pin.archive} has SHA-256 ${hash}, but scripts/uv-pins.json pins ${pin.sha256}. Not using it.`,
    );
  }
  const published = await publishedHash(url);
  if (published && published !== hash) {
    throw new Error(
      `${pin.archive} has SHA-256 ${hash}, but the release publishes ${published}. Not using it.`,
    );
  }
  if (!pin.sha256) {
    pin.sha256 = hash;
    writeFileSync(pinsPath, `${JSON.stringify(pins, null, 2)}\n`);
    console.log(
      `Pinned ${pin.archive} to ${hash}${published ? " (matches the release's .sha256)" : " (the release's .sha256 could not be read)"}. Commit scripts/uv-pins.json.`,
    );
  }

  const tmp = mkdtempSync(join(tmpdir(), "oxide-uv-"));
  try {
    const file = join(tmp, pin.archive);
    writeFileSync(file, archive);
    const unpacked = join(tmp, "unpacked");
    mkdirSync(unpacked);
    // bsdtar (Windows 10+, macOS) reads zip and tar.gz alike; GNU tar reads the tar.gz.
    execFileSync("tar", ["-xf", file, "-C", unpacked], { stdio: "inherit" });
    const found = findBinary(unpacked, exe);
    if (!found) throw new Error(`${pin.archive} has no ${exe}.`);
    mkdirSync(outDir, { recursive: true });
    copyFileSync(found, dest);
    if (!windows) chmodSync(dest, 0o755);
  } finally {
    rmSync(tmp, { recursive: true, force: true });
  }
  console.log(`uv ${pins.version} for ${triple}: ${dest}`);
  return true;
}

const args = parseArgs(process.argv.slice(2));
const pins = JSON.parse(readFileSync(pinsPath, "utf8"));
const triples = args.all
  ? Object.keys(pins.targets)
  : args.targets.length
    ? args.targets
    : [hostTriple()];
for (const triple of triples) {
  await fetchTarget(pins, triple, args.force, args.archive);
}
