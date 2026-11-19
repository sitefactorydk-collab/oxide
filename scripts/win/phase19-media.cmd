@echo off
chcp 65001 >nul
title Oxide - Phase 19 media generation
cd /d "%~dp0..\.."
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0phase19-media.ps1"
echo.
echo Logs: logs\phase19-media-prep-summary.log, logs\check-summary.log, logs\commit-summary.log. Oxide starts in its own window.
pause
