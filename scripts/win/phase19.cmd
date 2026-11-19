@echo off
chcp 65001 >nul
title Oxide - Phase 19
cd /d "%~dp0..\.."
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0phase19.ps1"
echo.
echo Logs: logs\phase19-prep-summary.log, logs\check-summary.log, logs\commit-summary.log. Oxide starts in its own window.
pause
