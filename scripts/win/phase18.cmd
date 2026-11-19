@echo off
chcp 65001 >nul
title Oxide - Phase 18
cd /d "%~dp0..\.."
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0phase18.ps1"
echo.
echo Logs: logs\check-summary.log, logs\npm-audit.txt. Oxide starts in its own window.
pause
