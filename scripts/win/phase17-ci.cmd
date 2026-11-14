@echo off
chcp 65001 >nul
title Oxide - Phase 17 CI fixes
cd /d "%~dp0..\.."
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0phase17-ci.ps1"
echo.
echo Done. Watch the new CI run on GitHub (Actions tab).
pause
