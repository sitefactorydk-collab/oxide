@echo off
chcp 65001 >nul
title Oxide - Phase 2 fetch
cd /d "%~dp0..\.."
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0phase2-fetch.ps1"
echo.
echo Done. You can close this window; Claude reads the logs.
pause
