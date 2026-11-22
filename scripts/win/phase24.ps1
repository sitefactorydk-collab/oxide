# Phase 24: compact tool history in the transcript (summary lines, compact rows,
# Settings -> Appearance -> Show tool steps).
#  1. Stops a running dev app; npm install; regenerates the bindings (src\bindings\index.ts was
#     edited by hand: logs\phase24-bindings.diff shows what the generator changed).
#  2. Every check (fmt, prettier, clippy, cargo test, typecheck, lint, Vitest) and vite build.
#  3. Commits with logs\commit-msg-phase24.txt only when all of that passed (no push).
#  4. Starts the app for the manual exit checks (the "organize this folder" chat in Auto).
# Logs: logs\phase24-prep*.log, logs\check*.log, logs\phase24-build*.log, logs\commit*.log.
. "$PSScriptRoot\_common.ps1"
Get-Process -Name oxide, oxide-mcp-mock -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Get-NetTCPConnection -LocalPort 1420 -State Listen -ErrorAction SilentlyContinue |
  ForEach-Object { Stop-Process -Id $_.OwningProcess -Force -ErrorAction SilentlyContinue }

Start-Log 'phase24-prep'
Run 'npm install' 'npm install --no-audit --no-fund'
Run 'bindings' 'npm run bindings'
Run 'prettier bindings' 'npx prettier --write src\bindings\index.ts --log-level warn'
cmd /c 'git diff --stat -- src/bindings/index.ts > logs\phase24-bindings.diff 2>&1'
cmd /c 'git diff -- src/bindings/index.ts >> logs\phase24-bindings.diff 2>&1'
End-Log
$prep = $script:Failures

& "$PSScriptRoot\check.ps1"
$checks = $LASTEXITCODE

Start-Log 'phase24-build'
Run 'vite build' 'npx vite build --logLevel warn'
End-Log
$build = $script:Failures

$msg = Join-Path $Root 'logs\commit-msg-phase24.txt'
if ($prep -eq 0 -and $checks -eq 0 -and $build -eq 0 -and (Test-Path $msg)) {
  Copy-Item -Force $msg (Join-Path $Root 'logs\commit-msg.txt')
  & "$PSScriptRoot\commit.ps1"
  Write-Host 'All green and committed. Starting Oxide for the manual checks (close its window when done).' -ForegroundColor Green
} else {
  Write-Host 'Something failed: see logs\phase24-prep-summary.log, check-summary.log, phase24-build-summary.log. Not committed. Starting Oxide anyway.' -ForegroundColor Red
}
Start-Process -FilePath (Join-Path $PSScriptRoot 'dev.cmd')
