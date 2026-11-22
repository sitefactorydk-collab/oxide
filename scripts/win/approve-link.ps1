# Scheduled page: "Open the chat to approve" on a task whose last run waits for approval.
# Every check, then a commit with logs\commit-msg-approve-link.txt when all passed.
. "$PSScriptRoot\_common.ps1"
Get-Process -Name oxide -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
& "$PSScriptRoot\check.ps1"
$checks = $LASTEXITCODE
$msg = Join-Path $Root 'logs\commit-msg-approve-link.txt'
if ($checks -eq 0 -and (Test-Path $msg)) {
  Copy-Item -Force $msg (Join-Path $Root 'logs\commit-msg.txt')
  & "$PSScriptRoot\commit.ps1"
  Write-Host 'All green and committed. Starting Oxide.' -ForegroundColor Green
} else {
  Write-Host 'Something failed: see logs\check-summary.log. Not committed. Starting Oxide anyway.' -ForegroundColor Red
}
Start-Process -FilePath (Join-Path $PSScriptRoot 'dev.cmd')
