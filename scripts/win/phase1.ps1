# Phase 1: stop any running dev app, install deps, regenerate icons, run all checks.
# Commits only if checks pass. Logs: logs\phase1*.log, logs\check*.log
. "$PSScriptRoot\_common.ps1"
Start-Log 'phase1'
Add-Content -Encoding utf8 -Path $script:SummaryFile -Value 'started'

# An old `tauri dev` keeps Vite on port 1420 and oxide.exe locked; stop both.
Get-Process -Name oxide -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Get-NetTCPConnection -LocalPort 1420 -State Listen -ErrorAction SilentlyContinue |
  ForEach-Object { Stop-Process -Id $_.OwningProcess -Force -ErrorAction SilentlyContinue }

Run 'npm install' 'npm install --no-fund --no-audit'
Run 'npm ls (top level)' 'npm ls --depth=0'
Run 'tauri icon' 'npx tauri icon app-icon.svg'
Run 'vite build' 'npx vite build'
End-Log

$commit = Test-Path (Join-Path $Root 'logs\commit-msg-phase1.txt')
& "$PSScriptRoot\check.ps1"
if ($LASTEXITCODE -eq 0 -and $commit) {
  Copy-Item -Force (Join-Path $Root 'logs\commit-msg-phase1.txt') (Join-Path $Root 'logs\commit-msg.txt')
  & "$PSScriptRoot\commit.ps1"
} elseif ($LASTEXITCODE -ne 0) {
  Write-Host 'Checks failed: not committing. See logs\check-summary.log' -ForegroundColor Red
} else {
  Write-Host 'Checks passed. Not committing yet (waiting for the screenshot review).' -ForegroundColor Yellow
}
