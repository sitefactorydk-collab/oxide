@echo off
chcp 65001 >nul
title Oxide - Phase 23 fixes
cd /d "%~dp0..\.."
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0phase23b.ps1"
echo.
echo Logs: logs\phase23b-prep-summary.log, logs\check-summary.log, logs\commit-summary.log. Oxide starts in its own window.
pause
