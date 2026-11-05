@echo off
chcp 65001 >nul
title Oxide - Phase 4 prep
cd /d "%~dp0..\.."
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0phase4-prep.ps1"
echo.
echo Done. You can close this window; Claude reads the logs.
pause
