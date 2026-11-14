# Phase 17: packaging, updates & CI.
#  1. The update signing key: made once in %USERPROFILE%\.tauri\oxide-updater.key (no password),
#     its public key written into src-tauri\tauri.conf.json (scripts\updater-key.mjs).
#  2. uv pins for every target (scripts\uv-pins.json), so CI checks each download's hash.
#     Informational: a failed download doesn't block the rest.
#  3. Bindings, the Chrome extension, every check (fmt, prettier, clippy, cargo test, typecheck,
#     lint, Vitest).
#  4. The Windows installers: `npx tauri build` (release build, NSIS + MSI, signed update files).
#  5. Commits with logs\commit-msg-phase17.txt when all of that passed, then opens the folder
#     with the installers.
# Logs: logs\phase17-prep*.log, logs\check*.log, logs\phase17-build*.log, logs\commit*.log.
. "$PSScriptRoot\_common.ps1"
Get-Process -Name oxide, oxide-mcp-mock -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Get-NetTCPConnection -LocalPort 1420 -State Listen -ErrorAction SilentlyContinue |
  ForEach-Object { Stop-Process -Id $_.OwningProcess -Force -ErrorAction SilentlyContinue }

Start-Log 'phase17-prep'
Run 'npm install' 'npm install --no-audit --no-fund'
Run 'updater key' 'node scripts\updater-key.mjs'
Run 'prettier tauri.conf.json' 'npx prettier --write src-tauri\tauri.conf.json'
Run 'bindings' 'npm run bindings'
Run 'extension' 'npm run extension'
End-Log
$prep = $script:Failures

# Pins for every uv target (informational).
Start-Log 'phase17-uv'
Run 'uv pins (all targets)' 'node scripts\fetch-uv.mjs --all'
End-Log

& "$PSScriptRoot\check.ps1"
$checks = $LASTEXITCODE

Start-Log 'phase17-build'
$key = Join-Path $env:USERPROFILE '.tauri\oxide-updater.key'
if (Test-Path $key) {
  $env:TAURI_SIGNING_PRIVATE_KEY = (Get-Content $key -Raw).Trim()
}
# The key has no password. Windows can't hold an empty TAURI_SIGNING_PRIVATE_KEY_PASSWORD, and
# with CI set the Tauri CLI takes a missing password as empty instead of asking for one.
$env:CI = 'true'
Run 'tauri build (installers)' 'npx tauri build'
Remove-Item Env:\TAURI_SIGNING_PRIVATE_KEY, Env:\CI -ErrorAction SilentlyContinue
$bundle = Join-Path $Root 'target\release\bundle'
$found = Get-ChildItem -Path $bundle -Recurse -Include *.exe, *.msi, *.sig -ErrorAction SilentlyContinue
Add-Content -Encoding utf8 -Path $script:LogFile -Value "`n=== bundle files ==="
foreach ($f in $found) {
  $line = '{0}  {1:N1} MB' -f $f.FullName.Substring($Root.Length + 1), ($f.Length / 1MB)
  Add-Content -Encoding utf8 -Path $script:LogFile -Value $line
  Add-Content -Encoding utf8 -Path $script:SummaryFile -Value "  $line"
  Write-Host "  $line"
}
End-Log
$build = $script:Failures

$msg = Join-Path $Root 'logs\commit-msg-phase17.txt'
if ($prep -eq 0 -and $checks -eq 0 -and $build -eq 0 -and (Test-Path $msg)) {
  Copy-Item -Force $msg (Join-Path $Root 'logs\commit-msg.txt')
  & "$PSScriptRoot\commit.ps1"
} else {
  Write-Host 'Something failed: not committing. See logs\phase17-prep-summary.log, check-summary.log and phase17-build-summary.log' -ForegroundColor Red
}
if (Test-Path $bundle) { Start-Process explorer.exe $bundle }
Write-Host ''
Write-Host "Update signing key: $key (back it up; it goes into the GitHub secret TAURI_SIGNING_PRIVATE_KEY, see docs\RELEASING.md)." -ForegroundColor Yellow
