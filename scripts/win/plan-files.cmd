@echo off
chcp 65001 >nul
title Oxide - plugin creators: plan, then file by file
cd /d "%~dp0..\.."
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0plan-files.ps1"
echo.
echo Logs: logs\plan-files-prep-summary.log, logs\check-summary.log, logs\commit-summary.log. Oxide starts in its own window.
pause
