# Phase 12b: stop any running dev app, regenerate the bindings, run all checks, check the
# production build, then the exit check against the real Docker engine (downloads the
# sandbox's images the first time: python:3.12-bookworm, about 400 MB, and mcp/filesystem),
# and commit when everything is green.
# Logs: logs\phase12b-prep*.log, logs\check*.log, logs\phase12b-build*.log,
# logs\phase12b-exit*.log, logs\commit*.log. The .cmd wrapper then starts the app.
. "$PSScriptRoot\_common.ps1"
Get-Process -Name oxide, oxide-mcp-mock -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Get-NetTCPConnection -LocalPort 1420 -State Listen -ErrorAction SilentlyContinue |
  ForEach-Object { Stop-Process -Id $_.OwningProcess -Force -ErrorAction SilentlyContinue }

Start-Log 'phase12b-prep'
Run 'bindings' 'npm run bindings'
End-Log
$prep = $script:Failures

& "$PSScriptRoot\check.ps1"
$checks = $LASTEXITCODE

Start-Log 'phase12b-build'
Run 'vite build' 'npx vite build --logLevel warn'
End-Log
$build = $script:Failures

# Exit check: projects built, tested, fixed and copied out in real sandboxes; no network, no
# host paths; time limits kill whole commands; reset removes everything.
Start-Log 'phase12b-exit'
Run 'docker running' 'docker version --format "client {{.Client.Version}} / engine {{.Server.Version}}"'
if ($script:Failures -eq 0) {
  Run 'download images' 'docker pull python:3.12-bookworm && docker pull mcp/filesystem'
  Run 'real docker: sandbox' 'cargo test -p oxide-sandbox --test real_docker -- --ignored --test-threads 1 --nocapture'
  Run 'real docker: chat' 'cargo test -p oxide-agent --lib sandbox_build_fix_and_copy_out_through_a_chat -- --ignored --nocapture'
} else {
  Add-Content -Encoding utf8 -Path $script:SummaryFile -Value 'real docker                  SKIPPED  (Docker is not running: start Docker Desktop and run this again)'
}
End-Log
$exit = $script:Failures

$msg = Join-Path $Root 'logs\commit-msg-phase12b.txt'
if ($prep -eq 0 -and $checks -eq 0 -and $build -eq 0 -and $exit -eq 0 -and (Test-Path $msg)) {
  Copy-Item -Force $msg (Join-Path $Root 'logs\commit-msg.txt')
  & "$PSScriptRoot\commit.ps1"
} else {
  Write-Host 'Something failed: not committing. See logs\phase12b-prep-summary.log, check-summary.log, phase12b-build-summary.log and phase12b-exit-summary.log' -ForegroundColor Red
}
