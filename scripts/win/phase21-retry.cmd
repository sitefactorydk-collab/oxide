@echo off
chcp 65001 >nul
title Oxide - Phase 21 (retry)
cd /d "%~dp0..\.."
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0phase21-retry.ps1"
echo.
echo Logs: logs\phase21-prep-summary.log, logs\check-summary.log, logs\phase21-build-summary.log, logs\commit-summary.log. Oxide starts in its own window.
pause
