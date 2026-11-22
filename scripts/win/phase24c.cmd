@echo off
chcp 65001 >nul
title Oxide - Phase 24
cd /d "%~dp0..\.."
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0phase24c.ps1"
echo.
echo Logs: logs\phase24c-focus-summary.log, logs\check-summary.log, logs\commit-summary.log. Oxide starts in its own window.
pause
