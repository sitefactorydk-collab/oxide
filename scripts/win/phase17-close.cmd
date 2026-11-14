@echo off
chcp 65001 >nul
title Oxide - Phase 17 close-out
cd /d "%~dp0..\.."
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0phase17-close.ps1"
echo.
pause
