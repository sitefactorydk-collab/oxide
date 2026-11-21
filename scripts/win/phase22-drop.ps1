# Phase 22 follow-up: files dropped on a Utility App reach the app (Translator, Plugin Creator,
# HTML Table from Image). New command read_dropped_files, so the bindings are regenerated
# (src\bindings\index.ts was edited by hand; logs\phase22-drop-bindings.diff shows what the
# generator changed). Then every check and vite build; commits with
# logs\commit-msg-phase22-drop.txt only when all pass; then the app.
. "$PSScriptRoot\_common.ps1"
Get-Process -Name oxide, oxide-mcp-mock -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Get-NetTCPConnection -LocalPort 1420 -State Listen -ErrorAction SilentlyContinue |
  ForEach-Object { Stop-Process -Id $_.OwningProcess -Force -ErrorAction SilentlyContinue }

Start-Log 'phase22-drop-prep'
Run 'bindings' 'npm run bindings'
Run 'prettier bindings' 'npx prettier --write src\bindings\index.ts --log-level warn'
cmd /c 'git diff -- src/bindings/index.ts > logs\phase22-drop-bindings.diff 2>&1'
End-Log
$prep = $script:Failures

& "$PSScriptRoot\check.ps1"
$checks = $LASTEXITCODE

Start-Log 'phase22-drop-build'
Run 'vite build' 'npx vite build --logLevel warn'
End-Log
$build = $script:Failures

$msg = Join-Path $Root 'logs\commit-msg-phase22-drop.txt'
if ($prep -eq 0 -and $checks -eq 0 -and $build -eq 0 -and (Test-Path $msg)) {
  Copy-Item -Force $msg (Join-Path $Root 'logs\commit-msg.txt')
  & "$PSScriptRoot\commit.ps1"
  Write-Host 'All green and committed. Starting Oxide.' -ForegroundColor Green
} else {
  Write-Host 'Something failed: see logs\phase22-drop-prep-summary.log, check-summary.log, phase22-drop-build-summary.log. Not committed. Starting Oxide anyway.' -ForegroundColor Red
}
Start-Process -FilePath (Join-Path $PSScriptRoot 'dev.cmd')
