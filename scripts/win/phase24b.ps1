# Phase 24, second run: the two tests that failed in phase24.cmd, alone (three times, to see
# whether the permission picker test is a timing flake), then every check and the commit.
# Logs: logs\phase24b-focus*.log, logs\check*.log, logs\commit*.log.
. "$PSScriptRoot\_common.ps1"
Get-Process -Name oxide, oxide-mcp-mock -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Get-NetTCPConnection -LocalPort 1420 -State Listen -ErrorAction SilentlyContinue |
  ForEach-Object { Stop-Process -Id $_.OwningProcess -Force -ErrorAction SilentlyContinue }

Start-Log 'phase24b-focus'
foreach ($i in 1..3) {
  Run "focused vitest $i" 'npx vitest run src/features/permissions/Permissions.test.tsx src/app/App.test.tsx src/features/tools/ToolGroup.test.tsx'
}
End-Log

& "$PSScriptRoot\check.ps1"
$checks = $LASTEXITCODE

$msg = Join-Path $Root 'logs\commit-msg-phase24.txt'
if ($checks -eq 0 -and (Test-Path $msg)) {
  Copy-Item -Force $msg (Join-Path $Root 'logs\commit-msg.txt')
  & "$PSScriptRoot\commit.ps1"
  Write-Host 'All green and committed. Starting Oxide for the manual checks (close its window when done).' -ForegroundColor Green
} else {
  Write-Host 'Something failed: see logs\phase24b-focus-summary.log and logs\check-summary.log. Not committed. Starting Oxide anyway.' -ForegroundColor Red
}
Start-Process -FilePath (Join-Path $PSScriptRoot 'dev.cmd')
