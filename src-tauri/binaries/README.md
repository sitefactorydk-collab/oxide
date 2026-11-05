# Sidecar binaries

Tauri sidecars live here, named `<name>-<target-triple>[.exe]`
(for example `uv-x86_64-pc-windows-msvc.exe`).

They are not committed. Phase 12 adds a build script that downloads
checksum-pinned `uv` releases for each target into this folder.
