# Phase 8: stop any running dev app, install the new npm packages (mermaid, @babel/standalone,
# @tailwindcss/browser), run all checks, check that the production build makes the artifact
# frame, and commit when everything is green. Logs: logs\phase8*.log, logs\check*.log,
# logs\commit*.log. The .cmd wrapper then starts the app.
. "$PSScriptRoot\_common.ps1"
Get-Process -Name oxide, oxide-mcp-mock -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Get-NetTCPConnection -LocalPort 1420 -State Listen -ErrorAction SilentlyContinue |
  ForEach-Object { Stop-Process -Id $_.OwningProcess -Force -ErrorAction SilentlyContinue }

Start-Log 'phase8'
Run 'npm install' 'npm install --no-fund --no-audit'
End-Log
$installFailed = $script:Failures

& "$PSScriptRoot\check.ps1"
$checks = $LASTEXITCODE

Start-Log 'phase8-build'
Run 'vite build' 'npx vite build --logLevel warn'
$frame = Join-Path $Root 'dist\artifact-frame.html'
if (Test-Path $frame) {
  Add-Content -Encoding utf8 -Path $script:SummaryFile -Value 'artifact-frame.html          PASS  (in dist)'
} else {
  Add-Content -Encoding utf8 -Path $script:SummaryFile -Value 'artifact-frame.html          FAIL  (missing from dist)'
  $script:Failures++
}
End-Log
$build = $script:Failures

$msg = Join-Path $Root 'logs\commit-msg-phase8.txt'
if ($installFailed -eq 0 -and $checks -eq 0 -and $build -eq 0 -and (Test-Path $msg)) {
  Copy-Item -Force $msg (Join-Path $Root 'logs\commit-msg.txt')
  & "$PSScriptRoot\commit.ps1"
} else {
  Write-Host 'Something failed: not committing. See logs\check-summary.log and logs\phase8-build-summary.log' -ForegroundColor Red
}
