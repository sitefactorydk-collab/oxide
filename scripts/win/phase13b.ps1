# Phase 13b: context management & chat compaction. Regenerates the bindings, runs every
# check (fmt, prettier, clippy, cargo test, typecheck, lint, Vitest) and vite build, and
# saves what Strata and LM Studio report about their models (to see whether they name a
# context window). Commits when everything is green.
# Logs: logs\phase13b-prep*.log, logs\check*.log, logs\phase13b-build*.log,
# logs\phase13b-models.json, logs\commit*.log. The .cmd wrapper then starts the app.
. "$PSScriptRoot\_common.ps1"
Get-Process -Name oxide, oxide-mcp-mock -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Get-NetTCPConnection -LocalPort 1420 -State Listen -ErrorAction SilentlyContinue |
  ForEach-Object { Stop-Process -Id $_.OwningProcess -Force -ErrorAction SilentlyContinue }

Start-Log 'phase13b-prep'
Run 'bindings' 'npm run bindings'
End-Log
$prep = $script:Failures

& "$PSScriptRoot\check.ps1"
$checks = $LASTEXITCODE

Start-Log 'phase13b-build'
Run 'vite build' 'npx vite build --logLevel warn'
End-Log
$build = $script:Failures

# What local servers say about their models (informational: never fails the run).
$models = [ordered]@{}
foreach ($c in @(
    @{ name = 'strata'; url = 'http://127.0.0.1:47831/v1/models' },
    @{ name = 'lmstudio'; url = 'http://127.0.0.1:1234/api/v0/models' },
    @{ name = 'lmstudio-v1'; url = 'http://127.0.0.1:1234/v1/models' })) {
  try {
    $models[$c.name] = Invoke-RestMethod -Uri $c.url -TimeoutSec 5
  } catch {
    $models[$c.name] = "not reachable: $($_.Exception.Message)"
  }
}
$models | ConvertTo-Json -Depth 10 | Set-Content -Encoding utf8 (Join-Path $Root 'logs\phase13b-models.json')

$msg = Join-Path $Root 'logs\commit-msg-phase13b.txt'
if ($prep -eq 0 -and $checks -eq 0 -and $build -eq 0 -and (Test-Path $msg)) {
  Copy-Item -Force $msg (Join-Path $Root 'logs\commit-msg.txt')
  & "$PSScriptRoot\commit.ps1"
} else {
  Write-Host 'Something failed: not committing. See logs\phase13b-prep-summary.log, check-summary.log and phase13b-build-summary.log' -ForegroundColor Red
}
