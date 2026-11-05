# Oxide — Progress Log

The current phase and hand-off notes live here. Every phase chat reads this file first and appends to it last (template in PLAN.md §4).

## Status
- **Last completed:** Phase 0 — Scaffold & toolchain
- **Next phase:** Phase 1 — Design system & app shell (static)
- **Repo location:** `C:\AI Apps\Oxide` (git, branch `main`)

## How to build and verify (read this first, every phase)
- **Where builds run:** the Linux VM behind the desktop link has no Rust, and its proxy blocks crates.io and npm. **All builds run on Windows.** The agent writes files through the link, the user double-clicks a script in `scripts\win\`, and the agent reads `logs\*.log` back.
- **Computer use doesn't help here.** It returned blank screenshots on this PC, and Explorer/PowerShell are click-only for the agent anyway. Don't spend time on it. Ask the user to double-click instead.
- **Scripts** (see `scripts\win\README.md`):
  - `dev.cmd`: app with hot reload. Logs to `logs\dev.log`.
  - `check.cmd` / `check.ps1`: cargo fmt, prettier, clippy `-D warnings`, cargo test, typecheck, lint and vitest. Logs to `logs\check-summary.log` (PASS/FAIL table) and `logs\check.log` (full output).
  - `commit.ps1`: `git add -A` + commit with the message in `logs\commit-msg.txt`.
- **Pattern for each phase:** write a `scripts\win\phaseN.cmd/.ps1` that runs any setup the phase needs (new npm/cargo deps), then `check.ps1`, then `commit.ps1` only if checks pass, then `dev.cmd`. Write `logs\commit-msg.txt` and the final PROGRESS.md *before* asking the user to run it. Then the user double-clicks once.
- **Line endings:** write `.cmd` files with CRLF (`sed -i 's/$/\r/'`). Everything else uses LF.
- **Deleting files:** `rm` in the linked folder needs delete permission (`device_request_delete_permission`). Never run git from the Linux VM; use `commit.ps1` on Windows.

## Phase 0 — Scaffold & toolchain — 2026-11-05
Status: done. The app window opened and showed the `ping` reply. Final checks and the commit run through `phase0-finish.cmd`; see `logs\check-summary.log` and `logs\commit-summary.log`.

Toolchain (from `logs\toolchain.log`):
- Windows 11 Pro (10.0.26300)
- rustc/cargo 1.99.1, MSVC target; rustup 1.29.2
- tauri-cli 2.12.4
- node 26.13.0, npm 12.0.2
- git 2.54.0, uv 0.12.10
- Visual Studio Community 2022 17.14 (C++ tools)
- WebView2 156.0.4314.58

Done:
- **Cargo workspace:**
  - Edition 2024, resolver 3, `rust-version = "1.90"`.
  - Shared deps; lints deny `unwrap_used`, `dbg_macro` and `unsafe_code`.
  - `rust-toolchain.toml` (stable + clippy/rustfmt).
- **8 stub crates** under `crates/`:
  - `oxide-core` has `APP_NAME` and `version()` plus tests.
  - The others hold module docs only, naming the phase that fills them.
- **`src-tauri` app crate** (`oxide`, lib `oxide_lib`):
  - `ping` command registered through tauri-specta.
  - Bindings are exported to `src/bindings/index.ts` on every debug run, and by the test `export_bindings` (`npm run bindings`).
  - Logging via `tracing`.
  - `tauri.conf.json`: id `dev.oxide.app`, window 1200×800 with a 760×520 minimum, strict CSP.
  - `capabilities/default.json` holds `core:default`.
- **Windows manifest fix:**
  - `src-tauri/windows-app-manifest.xml` is embedded into *every* binary by `build.rs` (`/MANIFEST:EMBED`).
  - `tauri-build` is told not to add its own (`WindowsAttributes::new_without_app_manifest()`).
  - Without this, Rust test binaries crash on start with `STATUS_ENTRYPOINT_NOT_FOUND` (0xc0000139).
- **Frontend:**
  - Vite 7, React 19, TS strict with `noUncheckedIndexedAccess`, and the `@/` alias.
  - Tailwind v4 via `@tailwindcss/vite`, with tokens in `src/styles/tokens.css` mapped through `@theme inline`.
  - ESLint 9 flat config, Prettier, Vitest + Testing Library (jsdom).
  - `App.tsx` calls `commands.ping`.
- **Icons:** placeholder `app-icon.svg` (rust tile + ring); `npx tauri icon` generated `src-tauri/icons/*`.
- **Docs:** `CLAUDE.md` (conventions), `PLAN.md` and `PROGRESS.md` in the repo root.

Deviations from PLAN.md:
- React 19 instead of 18: it's the current stable release, and nothing in the plan depends on 18.
- `rust-version` is 1.90, not 1.85. Cargo's MSRV-aware resolver had held Tauri at 2.11 to match 1.85, which mismatched `@tauri-apps/api` 2.12. Tauri 2.12 needs Rust 1.90.
- tauri-specta stays pinned at `=2.0.0-rc.21` with `specta =2.0.0-rc.22` and `specta-typescript 0.0.9`. Newer RCs exist (tauri-specta rc.26, specta rc.28, specta-typescript 0.0.15), but rc.21 works and its export API is the one in use. Upgrade deliberately in a later phase if something needs it.
- Windows dev scripts were added (not in the plan) because builds can't run from the agent side.

Known issues / TODO carried forward:
- npm 12 blocked esbuild's postinstall script (`allowScripts`). Vite works because esbuild ships its binary as an optional package. Revisit if esbuild ever fails to start.
- `eslint@9` reports as no longer supported (ESLint 10 is out). Upgrade together with the plugins in a later phase.
- Default git identity: `commit.ps1` sets a repo-local placeholder (`Oxide Developer <oxide-dev@localhost>`) only if none is configured.
- The placeholder icon is temporary. Phase 1 designs the real Oxide mark.

Notes for Phase 1:
- Build the design system in `src/styles/tokens.css`, using the `--ox-*` variable prefix and Tailwind mapping in `src/styles/index.css`.
- Add these npm deps in `scripts\win\phase1.ps1`: react-router, zustand, the Radix primitives, and lucide-react for icons (ISC licence; no Anthropic assets).
- Bundle fonts locally (Inter, Source Serif 4, JetBrains Mono, all OFL) in `src/assets/fonts/` via `@fontsource-variable/*` packages, so there's no network at runtime.
- For parity screenshots, ask the user to screenshot Claude desktop and Oxide side by side. Computer use can't capture this PC's screen.

Verified:
- Window opens with the `ping` reply ✅.
- clippy, cargo test, typecheck, lint and vitest pass via `phase0-finish.cmd`; see `logs\check-summary.log`.

## Planning — 2026-11-05
- Agreed scope and decisions (see PLAN.md §0). App name: Oxide.
- PLAN.md written and saved to the project.
