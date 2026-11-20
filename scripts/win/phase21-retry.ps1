# Phase 21 retry (prep already done by phase21.cmd): checks, build, commit, app.
# Phase 21: the Utility App builder (chat + Preview | Code, checkpoints, Save to Utility Apps).
#  1. Stops a running dev app; npm install; removes the Phase 20 CodeDialog (the builder
#     replaces it); regenerates the bindings (src\bindings\index.ts was edited by hand:
#     logs\phase21-bindings.diff shows what the generator changed) and the SDK types the
#     builder's model gets (src\appframe\oxide-sdk.d.ts).
#  2. Every check (fmt, prettier, clippy, cargo test, typecheck, lint, Vitest) and vite build.
#  3. Commits with logs\commit-msg-phase21.txt only when all of that passed (no push).
#  4. Starts the app for the manual exit checks (Utility Apps -> New app).
# Logs: logs\phase21-prep*.log, logs\check*.log, logs\phase21-build*.log, logs\commit*.log.
. "$PSScriptRoot\_common.ps1"
Get-Process -Name oxide, oxide-mcp-mock -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Get-NetTCPConnection -LocalPort 1420 -State Listen -ErrorAction SilentlyContinue |
  ForEach-Object { Stop-Process -Id $_.OwningProcess -Force -ErrorAction SilentlyContinue }

$prep = 0

& "$PSScriptRoot\check.ps1"
$checks = $LASTEXITCODE

Start-Log 'phase21-build'
Run 'vite build' 'npx vite build --logLevel warn'
End-Log
$build = $script:Failures

$msg = Join-Path $Root 'logs\commit-msg-phase21.txt'
if ($prep -eq 0 -and $checks -eq 0 -and $build -eq 0 -and (Test-Path $msg)) {
  Copy-Item -Force $msg (Join-Path $Root 'logs\commit-msg.txt')
  & "$PSScriptRoot\commit.ps1"
  Write-Host 'All green and committed. Starting Oxide for the manual checks (close its window when done).' -ForegroundColor Green
} else {
  Write-Host 'Something failed: see logs\phase21-prep-summary.log, check-summary.log, phase21-build-summary.log. Not committed. Starting Oxide anyway.' -ForegroundColor Red
}
Start-Process -FilePath (Join-Path $PSScriptRoot 'dev.cmd')
