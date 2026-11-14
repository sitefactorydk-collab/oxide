# Phase 17 fix: other OpenAI-compatible services (Groq) list their models. Raises the
# version to 0.1.1, runs every check, commits, pushes and tags v0.1.1, which starts the
# Release workflow (a new draft release). Publish v0.1.0 on GitHub before running this, so
# installed 0.1.0 copies can update to 0.1.1 once that draft is published too.
# Logs: logs\phase17-fix*.log, logs\check*.log, logs\commit*.log.
. "$PSScriptRoot\_common.ps1"
Get-Process -Name oxide -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
$version = '0.1.1'

Start-Log 'phase17-fix'
Run 'set version' "node scripts\set-version.mjs $version"
Run 'npm lock version' 'npm install --package-lock-only --no-audit --no-fund'
Run 'prettier' 'npx prettier --write package.json src-tauri\tauri.conf.json'
End-Log
$prep = $script:Failures

& "$PSScriptRoot\check.ps1"
$checks = $LASTEXITCODE

if ($prep -eq 0 -and $checks -eq 0) {
  Copy-Item -Force (Join-Path $Root 'logs\commit-msg-phase17-fix.txt') (Join-Path $Root 'logs\commit-msg.txt')
  & "$PSScriptRoot\commit.ps1"
  Start-Log 'phase17-fix-push'
  Run 'git push' 'git push origin main'
  Run 'git tag' "git tag v$version"
  Run 'git push tag' "git push origin v$version"
  End-Log
} else {
  Write-Host 'Something failed: not committing. See logs\phase17-fix-summary.log and check-summary.log' -ForegroundColor Red
}
