# Sidecar binaries

Tauri sidecars live here, named `<name>-<target-triple>[.exe]`
(for example `uv-x86_64-pc-windows-msvc.exe`). Nothing in this folder except this file is
committed.

## uv

Oxide manages its own Python with [`uv`](https://github.com/astral-sh/uv), bundled as the
sidecar `binaries/uv` (`tauri.conf.json` → `bundle.externalBin`).

```
npm run fetch-uv                 # the host's target
node scripts/fetch-uv.mjs --all  # every target in scripts/uv-pins.json
```

`scripts/uv-pins.json` names the pinned uv release and, per target, the SHA-256 of its archive.
A pin that is `null` is filled in the first time the archive is downloaded; after that an
archive with another hash is refused. Commit the file after the first download. To update uv,
change `version`, set the hashes back to `null`, run the script and commit the result.

`src-tauri/build.rs` warns when the sidecar for the build target is missing (and creates an empty
placeholder so `cargo check` and `cargo test` work); a release build fails instead.
