# Phase 14c: models create and edit skills (`save_skill`). Regenerates the bindings, runs
# every check (fmt, prettier, clippy, cargo test, typecheck, lint, Vitest) and vite build,
# then commits with logs\commit-msg-phase14c.txt when all of that is green.
# Logs: logs\phase14c-prep*.log, logs\check*.log, logs\phase14c-build*.log, logs\commit*.log.
# The .cmd wrapper then starts the app.
. "$PSScriptRoot\_common.ps1"
Get-Process -Name oxide, oxide-mcp-mock -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Get-NetTCPConnection -LocalPort 1420 -State Listen -ErrorAction SilentlyContinue |
  ForEach-Object { Stop-Process -Id $_.OwningProcess -Force -ErrorAction SilentlyContinue }

Start-Log 'phase14c-prep'
Run 'bindings' 'npm run bindings'
End-Log
$prep = $script:Failures

& "$PSScriptRoot\check.ps1"
$checks = $LASTEXITCODE

Start-Log 'phase14c-build'
Run 'vite build' 'npx vite build --logLevel warn'
End-Log
$build = $script:Failures

$msg = Join-Path $Root 'logs\commit-msg-phase14c.txt'
if ($prep -eq 0 -and $checks -eq 0 -and $build -eq 0 -and (Test-Path $msg)) {
  Copy-Item -Force $msg (Join-Path $Root 'logs\commit-msg.txt')
  & "$PSScriptRoot\commit.ps1"
} else {
  Write-Host 'Something failed: not committing. See logs\phase14c-prep-summary.log, check-summary.log and phase14c-build-summary.log' -ForegroundColor Red
}
