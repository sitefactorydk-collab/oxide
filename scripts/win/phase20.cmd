@echo off
chcp 65001 >nul
title Oxide - Phase 20
cd /d "%~dp0..\.."
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0phase20.ps1"
echo.
echo Logs: logs\phase20-prep-summary.log, logs\check-summary.log, logs\phase20-build-summary.log, logs\commit-summary.log. Oxide starts in its own window.
pause
