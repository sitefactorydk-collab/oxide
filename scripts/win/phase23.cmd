@echo off
chcp 65001 >nul
title Oxide - Phase 23
cd /d "%~dp0..\.."
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0phase23.ps1"
echo.
echo Logs: logs\phase23-prep-summary.log, logs\check-summary.log, logs\commit-summary.log. Oxide starts in its own window.
pause
