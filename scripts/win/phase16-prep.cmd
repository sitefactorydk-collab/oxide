@echo off
chcp 65001 >nul
title Oxide - Phase 16 prep
cd /d "%~dp0..\.."
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0phase16-prep.ps1"
echo.
pause
