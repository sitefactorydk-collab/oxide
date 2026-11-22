# Phase 25: the SP Page Builder Addon Creator and Elementor Widget Creator apps, and the SDK's
# `folder` capability.
#  1. Stops a running dev app; removes three stray files the agent left; regenerates the
#     bindings (src\bindings\index.ts was edited by hand: logs\creator-apps-bindings.diff shows
#     what the generator changed) and the SDK types (src\appframe\oxide-sdk.d.ts).
#  2. Every check (fmt, prettier, clippy, cargo test, typecheck, lint, Vitest) and vite build.
#  3. Commits the phase's files by name (the repo has untracked folders that must stay out)
#     with logs\commit-msg-creator-apps.txt, only when all of that passed (no push).
#  4. Starts the app for the manual checks (PLAN.md, Phase 25).
# Logs: logs\creator-apps-prep*.log, logs\check*.log, logs\creator-apps-build*.log, logs\commit*.log.
. "$PSScriptRoot\_common.ps1"
Get-Process -Name oxide, oxide-mcp-mock -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Get-NetTCPConnection -LocalPort 1420 -State Listen -ErrorAction SilentlyContinue |
  ForEach-Object { Stop-Process -Id $_.OwningProcess -Force -ErrorAction SilentlyContinue }

foreach ($stray in @('scripts\.sdk-tmp.mjs', 'src\appframe\zz-test.txt', 'src\appframe\sdk.ts.probe')) {
  Remove-Item -Force -ErrorAction SilentlyContinue (Join-Path $Root $stray)
}

Start-Log 'creator-apps-prep'
Run 'npm install' 'npm install --no-audit --no-fund'
Run 'bindings' 'npm run bindings'
Run 'prettier bindings' 'npx prettier --write src\bindings\index.ts --log-level warn'
cmd /c 'git diff --stat -- src/bindings/index.ts > logs\creator-apps-bindings.diff 2>&1'
cmd /c 'git diff -- src/bindings/index.ts >> logs\creator-apps-bindings.diff 2>&1'
# The generated SDK types file was read-only on this PC (EPERM on the first run).
cmd /c 'attrib -r src\appframe\oxide-sdk.d.ts > nul 2>&1'
$before = $script:Failures
Run 'sdk types' 'npm run sdk-types'
if ($script:Failures -gt $before) {
  # Still refused: replace the file instead of writing into it.
  Remove-Item -Force -ErrorAction SilentlyContinue (Join-Path $Root 'src\appframe\oxide-sdk.d.ts')
  $script:Failures = $before
  Run 'sdk types (new file)' 'npm run sdk-types'
}
End-Log
$prep = $script:Failures

& "$PSScriptRoot\check.ps1"
$checks = $LASTEXITCODE

Start-Log 'creator-apps-build'
Run 'vite build' 'npx vite build --logLevel warn'
End-Log
$build = $script:Failures

$msg = Join-Path $Root 'logs\commit-msg-creator-apps.txt'
$paths = @(
  'PLAN.md', 'PROGRESS.md', 'README.md',
  'crates/oxide-core/src/utility_app.rs',
  'crates/oxide-agent/src/apps', 'crates/oxide-agent/src/tools/files.rs',
  'src-tauri/capabilities/default.json', 'src-tauri/src/commands/apps.rs',
  'src-tauri/src/lib.rs', 'src-tauri/src/state.rs',
  'src/bindings/index.ts', 'src/appframe/sdk.ts', 'src/appframe/oxide-sdk.d.ts',
  'src/features/apps', 'src/test/fakeApps.ts',
  'utility-apps/sp-page-builder-addon-creator', 'utility-apps/elementor-widget-creator',
  'scripts/win/creator-apps.cmd', 'scripts/win/creator-apps.ps1'
)
if ($prep -eq 0 -and $checks -eq 0 -and $build -eq 0 -and (Test-Path $msg)) {
  Start-Log 'commit'
  Run 'git add' ("git add -- " + ($paths -join ' '))
  Run 'git commit' "git commit -F `"$msg`""
  Run 'git log' 'git log --oneline -n 3'
  Run 'git status' 'git status --short'
  End-Log
  Write-Host 'All green and committed. Starting Oxide for the manual checks (close its window when done).' -ForegroundColor Green
} else {
  Write-Host 'Something failed: see logs\creator-apps-prep-summary.log, check-summary.log, creator-apps-build-summary.log. Not committed. Starting Oxide anyway.' -ForegroundColor Red
}
Start-Process -FilePath (Join-Path $PSScriptRoot 'dev.cmd')
