@echo off
chcp 65001 >nul
title Oxide - Save button fix
cd /d "%~dp0..\.."
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0savebutton.ps1"
echo.
echo Logs: logs\check-summary.log, logs\commit-summary.log. Oxide starts in its own window.
pause
