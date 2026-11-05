# Phase 1 finish: run all checks, then commit with logs\commit-msg.txt if they pass.
. "$PSScriptRoot\_common.ps1"
& "$PSScriptRoot\check.ps1"
if ($LASTEXITCODE -eq 0) {
  & "$PSScriptRoot\commit.ps1"
} else {
  Write-Host 'Checks failed: not committing. See logs\check-summary.log' -ForegroundColor Red
}
