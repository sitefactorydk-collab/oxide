# Phase 5: stop any running dev app, install npm deps (no-op if current), run all checks,
# commit when green. Logs: logs\check*.log, logs\commit*.log. The .cmd wrapper then starts
# the app.
. "$PSScriptRoot\_common.ps1"
Get-Process -Name oxide, oxide-mcp-mock -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Get-NetTCPConnection -LocalPort 1420 -State Listen -ErrorAction SilentlyContinue |
  ForEach-Object { Stop-Process -Id $_.OwningProcess -Force -ErrorAction SilentlyContinue }

Start-Log 'phase5'
Run 'npm install' 'npm install --no-fund --no-audit'
Run 'node, npx and uvx for the MCP test servers' 'node --version && npx --version && uvx --version'
End-Log

& "$PSScriptRoot\check.ps1"
$msg = Join-Path $Root 'logs\commit-msg-phase5.txt'
if ($LASTEXITCODE -eq 0 -and (Test-Path $msg)) {
  Copy-Item -Force $msg (Join-Path $Root 'logs\commit-msg.txt')
  & "$PSScriptRoot\commit.ps1"
} elseif ($LASTEXITCODE -ne 0) {
  Write-Host 'Checks failed: not committing. See logs\check-summary.log' -ForegroundColor Red
}
