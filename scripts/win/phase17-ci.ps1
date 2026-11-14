# Phase 17 CI fixes: runs the oxide-agent tests, commits with logs\commit-msg-phase17-ci.txt
# when they pass, and pushes to GitHub (which starts the CI workflow again).
# Logs: logs\phase17-ci*.log, logs\commit*.log.
. "$PSScriptRoot\_common.ps1"
Start-Log 'phase17-ci'
Run 'cargo test oxide-agent' 'cargo test -p oxide-agent'
End-Log
if ($script:Failures -eq 0) {
  Copy-Item -Force (Join-Path $Root 'logs\commit-msg-phase17-ci.txt') (Join-Path $Root 'logs\commit-msg.txt')
  & "$PSScriptRoot\commit.ps1"
  Start-Log 'phase17-ci-push'
  Run 'git push' 'git push origin main'
  End-Log
} else {
  Write-Host 'Tests failed: not committing. See logs\phase17-ci.log' -ForegroundColor Red
}
