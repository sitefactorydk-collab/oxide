@echo off
chcp 65001 >nul
title Oxide - Phase 26: edit blocks everywhere
cd /d "%~dp0..\.."
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0phase26.ps1"
echo.
echo Logs: logs\phase26-prep-summary.log, logs\check-summary.log, logs\phase26-build-summary.log, logs\commit-summary.log. Oxide starts in its own window.
pause
