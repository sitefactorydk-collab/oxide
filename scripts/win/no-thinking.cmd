@echo off
chcp 65001 >nul
title Oxide - no thinking per file
cd /d "%~dp0..\.."
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0no-thinking.ps1"
echo.
echo Logs: logs\check-summary.log, logs\commit-summary.log. Oxide starts in its own window.
pause
