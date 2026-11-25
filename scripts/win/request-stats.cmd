@echo off
chcp 65001 >nul
title Oxide - app request timing
cd /d "%~dp0..\.."
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0request-stats.ps1"
echo.
echo Logs: logs\check-summary.log, logs\commit-summary.log. Oxide starts in its own window.
pause
