# Windows dev scripts

Double-click these in File Explorer. Each writes logs to `logs\` (git-ignored), which the agent reads back.

| Script | What it does | Logs |
|---|---|---|
| `dev.cmd` | Runs the app with hot reload (`cargo tauri dev`) | `dev.log` |
| `check.cmd` | All checks: fmt, prettier, clippy, cargo test, typecheck, lint, vitest | `check.log`, `check-summary.log` |
| `check-toolchain.cmd` | Records tool versions | `toolchain.log` |
| `commit.ps1` | Commits everything with the message in `logs\commit-msg.txt` (used by phase scripts) | `commit.log` |
| `phase0.cmd` | First-time setup for Phase 0 (already run) | `phase0*.log` |
| `phase0-finish.cmd` | Re-resolves deps, runs checks, commits Phase 0 when green, starts the app | `deps*`, `check*`, `commit*`, `dev.log` |

Later phases add a `phaseN.cmd` that does the phase's setup, runs `check.ps1`, commits when green and starts the app.
