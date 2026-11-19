# Maintenance between Phase 18 and 19: Vitest 5 (closes the dev-only npm audit findings).
# Node check, npm install, audit report, every check, vite build; commits with
# logs\commit-msg-vitest5.txt and pushes only when all of that passed.
# Logs: logs\vitest5*.log, logs\npm-audit-vitest5.txt, logs\check*.log, logs\commit*.log.
. "$PSScriptRoot\_common.ps1"
Get-Process -Name oxide, oxide-mcp-mock -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Get-NetTCPConnection -LocalPort 1420 -State Listen -ErrorAction SilentlyContinue |
  ForEach-Object { Stop-Process -Id $_.OwningProcess -Force -ErrorAction SilentlyContinue }

Start-Log 'vitest5'
Run 'node version (22.12+)' 'node -v && node -e "const v=process.versions.node.split(''.'').map(Number); process.exit(v[0]*1000+v[1] >= 22012 ? 0 : 1)"'
Run 'npm install' 'npm install --no-audit --no-fund'
Run 'npm ls vitest' 'npm ls vitest'
Run 'vite build' 'npx vite build --logLevel warn'
End-Log
$prep = $script:Failures
cmd /c 'npm audit > logs\npm-audit-vitest5.txt 2>&1'

& "$PSScriptRoot\check.ps1"
$checks = $LASTEXITCODE

$msg = Join-Path $Root 'logs\commit-msg-vitest5.txt'
if ($prep -eq 0 -and $checks -eq 0 -and (Test-Path $msg)) {
  Copy-Item -Force $msg (Join-Path $Root 'logs\commit-msg.txt')
  & "$PSScriptRoot\commit.ps1"
  if ($LASTEXITCODE -eq 0) {
    Start-Log 'vitest5-push'
    Run 'git push' 'git push origin main'
    End-Log
  }
} else {
  Write-Host 'Something failed: not committing. See logs\vitest5-summary.log and check-summary.log' -ForegroundColor Red
}
