@echo off
chcp 65001 >nul
title Oxide - Phase 24
cd /d "%~dp0..\.."
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0phase24b.ps1"
echo.
echo Logs: logs\phase24b-focus-summary.log, logs\check-summary.log, logs\commit-summary.log. Oxide starts in its own window.
pause
