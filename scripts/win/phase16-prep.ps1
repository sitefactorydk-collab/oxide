# Phase 16 prep: adds the input crate (enigo) and UI Automation (uiautomation, Windows) to oxide-computer, fetches it, and packs the
# sources of enigo and xcap into logs\p16-src.tgz so the agent can read their exact APIs
# (its Linux side has no crates.io). Then checks that oxide-computer still builds.
# Logs: logs\phase16-prep.log, logs\phase16-prep-summary.log. Commits nothing.
. "$PSScriptRoot\_common.ps1"
Start-Log 'phase16-prep'
Run 'cargo add enigo' 'cargo add -p oxide-computer enigo'
Run 'cargo add uiautomation' 'cargo add -p oxide-computer uiautomation --target "cfg(windows)"'
Run 'cargo fetch' 'cargo fetch'
Run 'cargo tree enigo' 'cargo tree -p oxide-computer --target x86_64-pc-windows-msvc -e normal --depth 1'

$tmp = Join-Path $env:TEMP 'oxide-p16-src'
if (Test-Path $tmp) { Remove-Item -Recurse -Force $tmp }
New-Item -ItemType Directory -Force -Path $tmp | Out-Null
$reg = Get-ChildItem (Join-Path $env:USERPROFILE '.cargo\registry\src') -Directory
foreach ($r in $reg) {
  Get-ChildItem $r.FullName -Directory | Where-Object { $_.Name -like 'enigo-*' -or $_.Name -like 'xcap-0.9*' -or $_.Name -like 'uiautomation*' -or $_.Name -like 'tao-0*' -or $_.Name -like 'tauri-2*' -or $_.Name -like 'tauri-runtime-wry-2*' -or $_.Name -like 'tauri-plugin-global-shortcut-2*' -or $_.Name -like 'global-hotkey-0*' } | ForEach-Object {
    if (-not (Test-Path (Join-Path $tmp $_.Name))) { Copy-Item -Recurse $_.FullName (Join-Path $tmp $_.Name) }
    Add-Content -Encoding utf8 -Path $script:LogFile -Value "copied $($_.Name)"
  }
}
$out = Join-Path $Root 'logs\p16-src.tgz'
if (Test-Path $out) { Remove-Item -Force $out }
Run 'tar' "tar -czf `"$out`" -C `"$tmp`" ."
Run 'cargo check computer' 'cargo check -p oxide-computer'
End-Log
