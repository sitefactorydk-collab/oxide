# Phase 26 follow-up: ai.edit keeps the edit blocks that match and asks only about the rest;
# the plugin creator apps send edit requests a smaller context.
#  1. Stops a running dev app. 2. Every check (check.ps1).
#  3. Commits the changed files by name with logs\commit-msg-edit-followup.txt when green.
#  4. Starts the app for the manual check.
. "$PSScriptRoot\_common.ps1"
Get-Process -Name oxide, oxide-mcp-mock -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Get-NetTCPConnection -LocalPort 1420 -State Listen -ErrorAction SilentlyContinue |
  ForEach-Object { Stop-Process -Id $_.OwningProcess -Force -ErrorAction SilentlyContinue }

& "$PSScriptRoot\check.ps1"
$checks = $LASTEXITCODE

$msg = Join-Path $Root 'logs\commit-msg-edit-followup.txt'
$paths = @(
  'PLAN.md', 'PROGRESS.md',
  'crates/oxide-core/src/edit.rs', 'crates/oxide-core/src/edit_tests.rs',
  'crates/oxide-agent/src/apps/text_edit.rs', 'crates/oxide-agent/src/apps/tests.rs',
  'utility-apps/sp-page-builder-addon-creator', 'utility-apps/elementor-widget-creator',
  'src/features/apps/CreatorApps.test.tsx',
  'scripts/win/docs26.cmd', 'scripts/win/docs26.ps1',
  'scripts/win/edit-followup.cmd', 'scripts/win/edit-followup.ps1'
)
if ($checks -eq 0 -and (Test-Path $msg)) {
  Start-Log 'commit'
  Run 'git add' ("git add -- " + ($paths -join ' '))
  Run 'git commit' "git commit -F `"$msg`""
  Run 'git log' 'git log --oneline -n 3'
  Run 'git status' 'git status --short'
  End-Log
  Write-Host 'All green and committed. Starting Oxide (close its window when done).' -ForegroundColor Green
} else {
  Write-Host 'Something failed: see logs\check-summary.log. Not committed. Starting Oxide anyway.' -ForegroundColor Red
}
Start-Process -FilePath (Join-Path $PSScriptRoot 'dev.cmd')
