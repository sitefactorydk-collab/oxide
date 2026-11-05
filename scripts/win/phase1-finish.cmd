@echo off
chcp 65001 >nul
title Oxide - Phase 1 finish
cd /d "%~dp0..\.."
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0phase1-finish.ps1"
echo.
pause
