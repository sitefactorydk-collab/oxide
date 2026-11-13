# Phase 16 follow-up 4: per-model pointer calibration, a grid on screenshots, and click results
# that name what was clicked. Regenerates the bindings, runs every
# check and vite build, then commits with logs\commit-msg-phase16-fix4.txt when all is green.
. "$PSScriptRoot\_common.ps1"
Get-Process -Name oxide, oxide-mcp-mock -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Get-NetTCPConnection -LocalPort 1420 -State Listen -ErrorAction SilentlyContinue |
  ForEach-Object { Stop-Process -Id $_.OwningProcess -Force -ErrorAction SilentlyContinue }

Start-Log 'phase16-fix4-prep'
Run 'bindings' 'npm run bindings'
End-Log
$prep = $script:Failures

& "$PSScriptRoot\check.ps1"
$checks = $LASTEXITCODE

Start-Log 'phase16-fix4-build'
Run 'real computer (ignored)' 'cargo test -p oxide-computer --test real_computer -- --ignored --nocapture'
Run 'vite build' 'npx vite build --logLevel warn'
End-Log
$build = $script:Failures

$msg = Join-Path $Root 'logs\commit-msg-phase16-fix4.txt'
if ($prep -eq 0 -and $checks -eq 0 -and $build -eq 0 -and (Test-Path $msg)) {
  Copy-Item -Force $msg (Join-Path $Root 'logs\commit-msg.txt')
  & "$PSScriptRoot\commit.ps1"
} else {
  Write-Host 'Something failed: not committing. See logs\phase16-fix4-prep-summary.log, check-summary.log and phase16-fix4-build-summary.log' -ForegroundColor Red
}
