@echo off
chcp 65001 >nul
title Oxide - Phase 22 drop fix
cd /d "%~dp0..\.."
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0phase22-drop.ps1"
echo.
echo Logs: logs\phase22-drop-prep-summary.log, logs\check-summary.log, logs\phase22-drop-build-summary.log, logs\commit-summary.log. Oxide starts in its own window.
pause
