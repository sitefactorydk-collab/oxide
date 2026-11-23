@echo off
chcp 65001 >nul
title Oxide - structured output for Utility Apps
cd /d "%~dp0..\.."
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0structured-output.ps1"
echo.
echo Logs: logs\check-summary.log, logs\commit-summary.log. Oxide starts in its own window.
pause
