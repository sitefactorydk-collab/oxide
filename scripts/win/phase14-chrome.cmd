@echo off
chcp 65001 >nul
title Oxide - Phase 14 real Chrome test
cd /d "%~dp0..\.."
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0phase14-chrome.ps1"
echo.
pause
