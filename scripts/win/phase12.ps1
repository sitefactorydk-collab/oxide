# Phase 12: stop any running dev app, fetch the pinned uv sidecar, write python\uv.lock when it
# is missing or older than pyproject.toml, regenerate the bindings, run all checks, check the
# production build, run the real-uv exit check (a fresh temporary profile goes from no Python
# to a chart: this downloads Python and the packages), and commit when everything is green.
# Logs: logs\phase12-prep*.log, logs\check*.log, logs\phase12-build*.log, logs\phase12-exit*.log,
# logs\commit*.log. The .cmd wrapper then starts the app.
. "$PSScriptRoot\_common.ps1"
Get-Process -Name oxide, oxide-mcp-mock -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Get-NetTCPConnection -LocalPort 1420 -State Listen -ErrorAction SilentlyContinue |
  ForEach-Object { Stop-Process -Id $_.OwningProcess -Force -ErrorAction SilentlyContinue }

$triple = (rustc -vV | Select-String '^host: (.+)$').Matches[0].Groups[1].Value.Trim()
$uvRel = "src-tauri\binaries\uv-$triple.exe"
$uvAbs = Join-Path $Root $uvRel

Start-Log 'phase12-prep'
Run 'fetch uv' 'node scripts\fetch-uv.mjs'
$lock = Join-Path $Root 'python\uv.lock'
$project = Join-Path $Root 'python\pyproject.toml'
if ((Test-Path $uvAbs) -and ((Get-Item $uvAbs).Length -gt 0)) {
  if (-not (Test-Path $lock) -or ((Get-Item $project).LastWriteTime -gt (Get-Item $lock).LastWriteTime)) {
    Run 'uv lock' "$uvRel lock --project python --python 3.12"
  }
} else {
  Add-Content -Encoding utf8 -Path $script:SummaryFile -Value 'uv lock                      SKIPPED  (no uv sidecar)'
}
Run 'bindings' 'npm run bindings'
End-Log
$prep = $script:Failures

& "$PSScriptRoot\check.ps1"
$checks = $LASTEXITCODE

Start-Log 'phase12-build'
Run 'vite build' 'npx vite build --logLevel warn'
End-Log
$build = $script:Failures

# Exit check: nothing installed -> Python, pandas, numpy, matplotlib, openpyxl -> a chart (PNG).
Start-Log 'phase12-exit'
$env:OXIDE_UV = $uvAbs
Run 'real uv: no python -> chart' 'cargo test -p oxide-pyenv --test real_uv -- --ignored --nocapture'
Remove-Item Env:\OXIDE_UV -ErrorAction SilentlyContinue
End-Log
$exit = $script:Failures

$msg = Join-Path $Root 'logs\commit-msg-phase12.txt'
if ($prep -eq 0 -and $checks -eq 0 -and $build -eq 0 -and $exit -eq 0 -and (Test-Path $msg)) {
  Copy-Item -Force $msg (Join-Path $Root 'logs\commit-msg.txt')
  & "$PSScriptRoot\commit.ps1"
} else {
  Write-Host 'Something failed: not committing. See logs\phase12-prep-summary.log, check-summary.log, phase12-build-summary.log and phase12-exit-summary.log' -ForegroundColor Red
}
