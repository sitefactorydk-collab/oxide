# Phase 26 follow-up 3: the plugin creator apps ask for no thinking on their per-file requests
# (ai.text / ai.edit `thinking: false`; LM Studio gets reasoning_effort "none" too).
#  1. Stops a running dev app. 2. Every check (check.ps1).
#  3. Commits the changed files by name with logs\commit-msg-no-thinking.txt when green.
#  4. Starts the app for the manual check.
. "$PSScriptRoot\_common.ps1"
Get-Process -Name oxide, oxide-mcp-mock -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Get-NetTCPConnection -LocalPort 1420 -State Listen -ErrorAction SilentlyContinue |
  ForEach-Object { Stop-Process -Id $_.OwningProcess -Force -ErrorAction SilentlyContinue }

cmd /c 'attrib -r src\appframe\oxide-sdk.d.ts > nul 2>&1'
npm run sdk-types
& "$PSScriptRoot\check.ps1"
$checks = $LASTEXITCODE

$msg = Join-Path $Root 'logs\commit-msg-no-thinking.txt'
$paths = @(
  'PROGRESS.md',
  'crates/oxide-llm/src/openai.rs',
  'crates/oxide-agent/src/apps/runtime.rs', 'crates/oxide-agent/src/apps/text_edit.rs',
  'crates/oxide-agent/src/apps/tests.rs',
  'src/appframe/sdk.ts', 'src/appframe/oxide-sdk.d.ts', 'src/features/apps/CreatorApps.test.tsx',
  'src/features/artifacts/Artifacts.test.tsx',
  'utility-apps/sp-page-builder-addon-creator', 'utility-apps/elementor-widget-creator',
  'scripts/win/no-thinking.cmd', 'scripts/win/no-thinking.ps1'
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
