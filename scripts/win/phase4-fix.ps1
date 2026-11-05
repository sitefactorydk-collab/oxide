# Phase 4 follow-up: PDF diagnostics, all checks, commit when green. The .cmd starts the app.
. "$PSScriptRoot\_common.ps1"
& "$PSScriptRoot\pdf-check.ps1"
Get-Process -Name oxide -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Get-NetTCPConnection -LocalPort 1420 -State Listen -ErrorAction SilentlyContinue |
  ForEach-Object { Stop-Process -Id $_.OwningProcess -Force -ErrorAction SilentlyContinue }
& "$PSScriptRoot\check.ps1"
$msg = Join-Path $Root 'logs\commit-msg-phase4-fix.txt'
if ($LASTEXITCODE -eq 0 -and (Test-Path $msg)) {
  Copy-Item -Force $msg (Join-Path $Root 'logs\commit-msg.txt')
  & "$PSScriptRoot\commit.ps1"
} elseif ($LASTEXITCODE -ne 0) {
  Write-Host 'Checks failed: not committing. See logs\check-summary.log' -ForegroundColor Red
}
