# Phase 0 first-time setup (already run once). Kept for reference / fresh clones.
. "$PSScriptRoot\_common.ps1"
& "$PSScriptRoot\check-toolchain.ps1"
Start-Log 'phase0'
if (-not (Test-Path (Join-Path $Root '.git'))) { Run 'git init' 'git init -b main' }
Run 'npm install' 'npm install --no-fund --no-audit'
Run 'tauri icon' 'npx tauri icon app-icon.svg'
Run 'cargo fetch' 'cargo fetch'
End-Log
& "$PSScriptRoot\check.ps1"
