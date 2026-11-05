# Runs every check from CLAUDE.md. Logs: logs\check.log, logs\check-summary.log
param([switch]$NoFormat)
. "$PSScriptRoot\_common.ps1"
Start-Log 'check'
if (-not $NoFormat) {
  Run 'cargo fmt' 'cargo fmt --all'
  Run 'prettier --write' 'npx prettier --write . --log-level warn'
}
Run 'cargo clippy' 'cargo clippy --workspace --all-targets -- -D warnings'
Run 'cargo test' 'cargo test --workspace'
Run 'npm run typecheck' 'npm run typecheck'
Run 'npm run lint' 'npm run lint'
Run 'npm test' 'npm test'
End-Log
exit $script:Failures
