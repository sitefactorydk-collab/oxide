@echo off
chcp 65001 >nul
title Oxide - Phase 25 (plugin creator apps)
cd /d "%~dp0..\.."
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0creator-apps.ps1"
echo.
echo Logs: logs\creator-apps-prep-summary.log, logs\check-summary.log, logs\creator-apps-build-summary.log, logs\commit-summary.log. Oxide starts in its own window.
pause
