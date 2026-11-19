# Phase 18 close-out: the npm audit fixes (KaTeX pinned to 0.18.10 by an override, minimatch
# via `npm audit fix`), a fresh audit report, every check and vite build; then commits with
# logs\commit-msg-phase18.txt and pushes (CI runs again) only when all of that passed.
# Logs: logs\phase18-close*.log, logs\npm-audit-after.txt, logs\check*.log, logs\commit*.log.
. "$PSScriptRoot\_common.ps1"
Get-Process -Name oxide, oxide-mcp-mock -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Get-NetTCPConnection -LocalPort 1420 -State Listen -ErrorAction SilentlyContinue |
  ForEach-Object { Stop-Process -Id $_.OwningProcess -Force -ErrorAction SilentlyContinue }

Start-Log 'phase18-close'
Run 'npm install' 'npm install --no-audit --no-fund'
# npm audit fix exits non-zero while the deferred Vitest findings remain, so it's informational.
cmd /c 'npm audit fix --no-fund > logs\npm-audit-fix.txt 2>&1'
Run 'npm ls katex' 'npm ls katex'
Run 'vite build' 'npx vite build --logLevel warn'
End-Log
$prep = $script:Failures
cmd /c 'npm audit > logs\npm-audit-after.txt 2>&1'

& "$PSScriptRoot\check.ps1"
$checks = $LASTEXITCODE

$msg = Join-Path $Root 'logs\commit-msg-phase18.txt'
if ($prep -eq 0 -and $checks -eq 0 -and (Test-Path $msg)) {
  Remove-Item -Force -ErrorAction SilentlyContinue (Join-Path $Root 'PHASE18-WIP.md')
  Copy-Item -Force $msg (Join-Path $Root 'logs\commit-msg.txt')
  & "$PSScriptRoot\commit.ps1"
  if ($LASTEXITCODE -eq 0) {
    Start-Log 'phase18-close-push'
    Run 'git push' 'git push origin main'
    End-Log
  }
} else {
  Write-Host 'Something failed (or logs\commit-msg-phase18.txt is missing): not committing. See logs\phase18-close-summary.log and check-summary.log' -ForegroundColor Red
}
