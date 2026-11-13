# Phase 16: computer use. Regenerates the bindings (this also builds src-tauri with
# oxide-computer's enigo, UI Automation and Win32 code for the first time), runs every check
# (fmt, prettier, clippy, cargo test, typecheck, lint, Vitest), the real-computer test (it moves
# the mouse pointer to the middle of each screen and back; it never clicks or types) and vite
# build, then commits with logs\commit-msg-phase16.txt when all of that is green.
# Logs: logs\phase16-prep*.log, logs\check*.log, logs\phase16-build*.log, logs\commit*.log.
# The .cmd wrapper then starts the app.
. "$PSScriptRoot\_common.ps1"
Get-Process -Name oxide, oxide-mcp-mock -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Get-NetTCPConnection -LocalPort 1420 -State Listen -ErrorAction SilentlyContinue |
  ForEach-Object { Stop-Process -Id $_.OwningProcess -Force -ErrorAction SilentlyContinue }

Start-Log 'phase16-prep'
Run 'bindings' 'npm run bindings'
End-Log
$prep = $script:Failures

& "$PSScriptRoot\check.ps1"
$checks = $LASTEXITCODE

Start-Log 'phase16-build'
Run 'real computer (ignored)' 'cargo test -p oxide-computer --test real_computer -- --ignored --nocapture'
Run 'vite build' 'npx vite build --logLevel warn'
End-Log
$build = $script:Failures

$msg = Join-Path $Root 'logs\commit-msg-phase16.txt'
if ($prep -eq 0 -and $checks -eq 0 -and $build -eq 0 -and (Test-Path $msg)) {
  # The hand-off note from the paused session isn't needed any more.
  Remove-Item -Force -ErrorAction SilentlyContinue (Join-Path $Root 'PHASE16-WIP.md')
  Copy-Item -Force $msg (Join-Path $Root 'logs\commit-msg.txt')
  & "$PSScriptRoot\commit.ps1"
} else {
  Write-Host 'Something failed: not committing. See logs\phase16-prep-summary.log, check-summary.log and phase16-build-summary.log' -ForegroundColor Red
}
