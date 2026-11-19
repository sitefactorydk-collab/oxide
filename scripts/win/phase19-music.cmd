@echo off
chcp 65001 >nul
title Oxide - Phase 19 music generation
cd /d "%~dp0..\.."
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0phase19-music.ps1"
echo.
echo Logs: logs\phase19-music-prep-summary.log, logs\check-summary.log, logs\commit-summary.log. Oxide starts in its own window.
pause
