# Phase 14 follow-up: page readers see content inside `display: contents` (Google results). Builds the extension,
# runs every check, commits with logs\commit-msg-phase14-reader.txt when all pass. The .cmd
# wrapper then starts the app. Afterwards reload the extension in chrome://extensions.
. "$PSScriptRoot\_common.ps1"
Get-Process -Name oxide, oxide-mcp-mock -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Get-NetTCPConnection -LocalPort 1420 -State Listen -ErrorAction SilentlyContinue |
  ForEach-Object { Stop-Process -Id $_.OwningProcess -Force -ErrorAction SilentlyContinue }

Start-Log 'phase14-reader-prep'
Run 'extension' 'npm run extension'
End-Log
$prep = $script:Failures

& "$PSScriptRoot\check.ps1"
$checks = $LASTEXITCODE

$msg = Join-Path $Root 'logs\commit-msg-phase14-reader.txt'
if ($prep -eq 0 -and $checks -eq 0 -and (Test-Path $msg)) {
  Copy-Item -Force $msg (Join-Path $Root 'logs\commit-msg.txt')
  & "$PSScriptRoot\commit.ps1"
} else {
  Write-Host 'Something failed: not committing. See logs\phase14-reader-prep-summary.log and check-summary.log' -ForegroundColor Red
}
