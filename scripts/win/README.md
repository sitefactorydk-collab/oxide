# Windows dev scripts

Double-click these in File Explorer. Each writes logs to `logs\` (git-ignored), which the agent reads back.

| Script | What it does | Logs |
|---|---|---|
| `dev.cmd` | Runs the app with hot reload (`cargo tauri dev`) | `dev.log` |
| `check.cmd` | All checks: fmt, prettier, clippy, cargo test, typecheck, lint, vitest | `check.log`, `check-summary.log` |
| `check-toolchain.cmd` | Records tool versions | `toolchain.log` |
| `commit.cmd` | Double-click wrapper for `commit.ps1` | `commit*` |
| `commit.ps1` | Commits everything with the message in `logs\commit-msg.txt` (used by phase scripts) | `commit.log` |
| `phase0.cmd` | First-time setup for Phase 0 (already run) | `phase0*.log` |
| `phase1.cmd` | Phase 1: stops a running dev app, `npm install`, regenerates icons, vite build, checks, then starts the app | `phase1*`, `check*`, `dev.log` |
| `phase1-finish.cmd` | Runs checks and commits Phase 1 with `logs\commit-msg.txt` | `check*`, `commit*` |
| `phase2-fetch.cmd` | Phase 2: `npm install`, bundles Rust crate sources for the agent's Linux-side builds (`logs\linux-vendor.tgz`), then runs the checks (no commit) | `phase2-fetch*`, `check*` |
| `phase2.cmd` | Phase 2: checks, commit when green, then starts the app | `check*`, `commit*`, `dev.log` |
| `phase3.cmd` | Phase 3: checks, commit when green, then starts the app | `check*`, `commit*`, `dev.log` |
| `phase4-prep.cmd` | Phase 4: `npm install`, refreshed Rust source bundle (`logs\linux-vendor.tgz`), and the local-server thinking test | `phase4-prep*`, `local-thinking.log` |
| `local-thinking.ps1` | Sends a tiny prompt to Strata and LM Studio with each thinking switch and logs which ones work (run by `phase4-prep.cmd`; can be run alone) | `local-thinking.log` |
| `phase4.cmd` | Phase 4: checks, commit when green, then starts the app | `check*`, `commit*`, `dev.log` |
| `phase5.cmd` | Phase 5: `npm install`, checks, commit when green, then starts the app | `phase5*`, `check*`, `commit*`, `dev.log` |
| `phase6.cmd` | Phase 6: checks, commit when green, then starts the app | `check*`, `commit*`, `dev.log` |
| `phase10.cmd` | Phase 10: checks, vite build, commit when green, then starts the app | `check*`, `phase10-build*`, `commit*`, `dev.log` |
| `phase12.cmd` | Phase 12: fetches the pinned uv, writes `python\uv.lock`, regenerates the bindings, checks, vite build, the real-uv exit check (downloads Python and the packages), commit when green, then starts the app | `phase12*`, `check*`, `commit*`, `dev.log` |
| `phase12b.cmd` | Phase 12b: regenerates the bindings, checks, vite build, the real-Docker exit check (needs Docker Desktop running; downloads `python:3.12-bookworm` and `mcp/filesystem` the first time), commit when green, then starts the app | `phase12b*`, `check*`, `commit*`, `dev.log` |
| `phase13.cmd` | Phase 13: `uv lock` (new `browser` group), bindings, checks, vite build, the real-browser exit check (installs Python and the browser packages into `target\browser-test-python`; uses Edge/Chrome; runs Browser-Use too when Strata or LM Studio is running), the agent's Python tests, commit when green, then starts the app | `phase13*`, `check*`, `commit*`, `dev.log` |
| `phase13-fix.cmd` | Phase 13 fix: a new chat's “+” menu switches are saved with the chat; bindings, checks, commit when green, then starts the app | `phase13-fix*`, `check*`, `commit*`, `dev.log` |
| `phase12-fix.cmd` | Phase 12 fix (Python status after setup): checks, commit when green, then starts the app | `check*`, `commit*`, `dev.log` |
| `phase0-finish.cmd` | Re-resolves deps, runs checks, commits Phase 0 when green, starts the app | `deps*`, `check*`, `commit*`, `dev.log` |

Later phases add a `phaseN.cmd` that does the phase's setup, runs `check.ps1`, commits when green and starts the app.
