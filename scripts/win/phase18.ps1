# Phase 18: hardening & polish (+ usage dashboard, HTML export, dictation). No commit yet:
#  1. Stops a running dev app; npm install; regenerates the bindings (src\bindings\index.ts was
#     edited by hand: logs\phase18-bindings.diff shows what the generator changed).
#  2. npm audit (informational): logs\npm-audit.txt and logs\npm-audit.json for the triage.
#  3. Every check (fmt, prettier, clippy, cargo test, typecheck, lint, Vitest) and vite build.
#  4. Starts the app for the manual checks (PHASE18-WIP.md). phase18-close.cmd commits later.
# Logs: logs\phase18-prep*.log, logs\check*.log, logs\phase18-build*.log, logs\dev.log.
. "$PSScriptRoot\_common.ps1"
Get-Process -Name oxide, oxide-mcp-mock -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Get-NetTCPConnection -LocalPort 1420 -State Listen -ErrorAction SilentlyContinue |
  ForEach-Object { Stop-Process -Id $_.OwningProcess -Force -ErrorAction SilentlyContinue }

Start-Log 'phase18-prep'
Run 'npm install' 'npm install --no-audit --no-fund'
Run 'bindings' 'npm run bindings'
Run 'prettier bindings' 'npx prettier --write src\bindings\index.ts --log-level warn'
cmd /c 'git diff --stat -- src/bindings/index.ts > logs\phase18-bindings.diff 2>&1'
cmd /c 'git diff -- src/bindings/index.ts >> logs\phase18-bindings.diff 2>&1'
End-Log
$prep = $script:Failures

# npm audit exits non-zero when it finds anything, so it's not a pass/fail step.
cmd /c 'npm audit > logs\npm-audit.txt 2>&1'
cmd /c 'npm audit --json > logs\npm-audit.json 2>nul'
cmd /c 'npm audit --omit=dev >> logs\npm-audit.txt 2>&1'
Write-Host 'npm audit: logs\npm-audit.txt' -ForegroundColor Yellow

& "$PSScriptRoot\check.ps1"
$checks = $LASTEXITCODE

Start-Log 'phase18-build'
Run 'vite build' 'npx vite build --logLevel warn'
End-Log
$build = $script:Failures

if ($prep -eq 0 -and $checks -eq 0 -and $build -eq 0) {
  Write-Host 'All green. Starting Oxide for the manual checks (close its window when done).' -ForegroundColor Green
} else {
  Write-Host 'Something failed: see logs\phase18-prep-summary.log, check-summary.log, phase18-build-summary.log. Starting Oxide anyway.' -ForegroundColor Red
}
Start-Process -FilePath (Join-Path $PSScriptRoot 'dev.cmd')
