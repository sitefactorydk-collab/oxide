# Phase 6: stop any running dev app, run all checks, commit when green. Logs: logs\check*.log,
# logs\commit*.log. The .cmd wrapper then starts the app.
. "$PSScriptRoot\_common.ps1"
Get-Process -Name oxide, oxide-mcp-mock -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Get-NetTCPConnection -LocalPort 1420 -State Listen -ErrorAction SilentlyContinue |
  ForEach-Object { Stop-Process -Id $_.OwningProcess -Force -ErrorAction SilentlyContinue }

& "$PSScriptRoot\check.ps1"
$msg = Join-Path $Root 'logs\commit-msg-phase6.txt'
if ($LASTEXITCODE -eq 0 -and (Test-Path $msg)) {
  Copy-Item -Force $msg (Join-Path $Root 'logs\commit-msg.txt')
  & "$PSScriptRoot\commit.ps1"
} elseif ($LASTEXITCODE -ne 0) {
  Write-Host 'Checks failed: not committing. See logs\check-summary.log' -ForegroundColor Red
}
