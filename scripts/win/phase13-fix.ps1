# Phase 13 fix: a new chat's "+" menu switches (e.g. Browser off) are saved with the chat, so
# its first reply follows them. Stops a running dev app, regenerates the bindings, runs all
# checks, and commits when green. Logs: logs\phase13-fix*.log, logs\check*.log, logs\commit*.log.
. "$PSScriptRoot\_common.ps1"
Get-Process -Name oxide, oxide-mcp-mock -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Get-NetTCPConnection -LocalPort 1420 -State Listen -ErrorAction SilentlyContinue |
  ForEach-Object { Stop-Process -Id $_.OwningProcess -Force -ErrorAction SilentlyContinue }

Start-Log 'phase13-fix'
Run 'bindings' 'npm run bindings'
End-Log
$prep = $script:Failures

& "$PSScriptRoot\check.ps1"
$checks = $LASTEXITCODE

$msg = Join-Path $Root 'logs\commit-msg-phase13-fix.txt'
if ($prep -eq 0 -and $checks -eq 0 -and (Test-Path $msg)) {
  Copy-Item -Force $msg (Join-Path $Root 'logs\commit-msg.txt')
  & "$PSScriptRoot\commit.ps1"
} else {
  Write-Host 'Something failed: not committing. See logs\phase13-fix-summary.log and check-summary.log' -ForegroundColor Red
}
