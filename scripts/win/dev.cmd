@echo off
chcp 65001 >nul
title Oxide - dev
cd /d "%~dp0..\.."
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
set NO_COLOR=1
set CARGO_TERM_COLOR=never
if not exist logs mkdir logs
if exist logs\dev.log del logs\dev.log
echo Starting Oxide in dev mode. Output is also written to logs\dev.log
echo Close this window (or press Ctrl+C, then Y) to stop the app.
echo.
where cargo-tauri >nul 2>nul
if %errorlevel%==0 (set "TAURI_CMD=cargo tauri dev") else (set "TAURI_CMD=npm run tauri -- dev")
powershell -NoProfile -Command "[Console]::OutputEncoding=[Text.Encoding]::UTF8; cmd /c '%TAURI_CMD% 2>&1' | ForEach-Object { Write-Host $_; Add-Content -Encoding utf8 -Path logs\dev.log -Value $_ }"
pause
