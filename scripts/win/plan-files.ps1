# The plugin creator apps plan first, then write one file per request (plain text), and
# the SDK's ai.text reports `truncated`.
#  1. Stops a running dev app; regenerates the SDK types (sdk.ts changed).
#  2. Every check (check.ps1).
#  3. Commits the changed files by name with logs\commit-msg-plan-files.txt when green.
#  4. Starts the app for the manual check (a new addon with Gemma in LM Studio).
. "$PSScriptRoot\_common.ps1"
Get-Process -Name oxide, oxide-mcp-mock -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Get-NetTCPConnection -LocalPort 1420 -State Listen -ErrorAction SilentlyContinue |
  ForEach-Object { Stop-Process -Id $_.OwningProcess -Force -ErrorAction SilentlyContinue }

Start-Log 'plan-files-prep'
cmd /c 'attrib -r src\appframe\oxide-sdk.d.ts > nul 2>&1'
Run 'sdk types' 'npm run sdk-types'
End-Log
$prep = $script:Failures

& "$PSScriptRoot\check.ps1"
$checks = $LASTEXITCODE

$msg = Join-Path $Root 'logs\commit-msg-plan-files.txt'
$paths = @(
  'PROGRESS.md',
  'crates/oxide-agent/src/apps/runtime.rs', 'crates/oxide-agent/src/apps/tests.rs',
  'src/appframe/sdk.ts', 'src/appframe/oxide-sdk.d.ts', 'src/features/apps/CreatorApps.test.tsx',
  'utility-apps/sp-page-builder-addon-creator', 'utility-apps/elementor-widget-creator',
  'scripts/win/plan-files.cmd', 'scripts/win/plan-files.ps1'
)
if ($prep -eq 0 -and $checks -eq 0 -and (Test-Path $msg)) {
  Start-Log 'commit'
  Run 'git add' ("git add -- " + ($paths -join ' '))
  Run 'git commit' "git commit -F `"$msg`""
  Run 'git log' 'git log --oneline -n 3'
  Run 'git status' 'git status --short'
  End-Log
  Write-Host 'All green and committed. Starting Oxide (close its window when done).' -ForegroundColor Green
} else {
  Write-Host 'Something failed: see logs\plan-files-prep-summary.log and logs\check-summary.log. Not committed. Starting Oxide anyway.' -ForegroundColor Red
}
Start-Process -FilePath (Join-Path $PSScriptRoot 'dev.cmd')
