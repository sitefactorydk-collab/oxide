# PowerShell 7's concise error records count as failures (CI failed on GitHub's Windows
# runner for v1.0.1). Every check, then the commit, then the push (CI runs again).
# Logs: logs\check*.log, logs\commit*.log.
. "$PSScriptRoot\_common.ps1"
& "$PSScriptRoot\check.ps1"
$checks = $LASTEXITCODE
$msg = Join-Path $Root 'logs\commit-msg-pwsh7.txt'
if ($checks -eq 0 -and (Test-Path $msg)) {
  # Only this fix: the untracked "Claude outputs" folder stays out of the commit.
  git add crates/oxide-agent/src/tools/shell.rs scripts/win/fix-pwsh7.ps1 scripts/win/fix-pwsh7.cmd
  git commit -F $msg
  if ($LASTEXITCODE -ne 0) { Write-Host 'The commit failed. Not pushed.' -ForegroundColor Red; exit 1 }
  Write-Host 'All green and committed. Pushing main (CI runs again on GitHub).' -ForegroundColor Green
  git push origin main
  Write-Host "Push exit code: $LASTEXITCODE"
} else {
  Write-Host 'Something failed: see logs\check-summary.log. Not committed, not pushed.' -ForegroundColor Red
}
