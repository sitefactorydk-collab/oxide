@echo off
chcp 65001 >nul
title Oxide - Vitest 5
cd /d "%~dp0..\.."
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0vitest5.ps1"
echo.
pause
