@echo off
chcp 65001 >nul
title Oxide - Phase 17 fix (0.1.1)
cd /d "%~dp0..\.."
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0phase17-fix.ps1"
echo.
echo Done. The Release workflow for v0.1.1 is now running on GitHub.
pause
