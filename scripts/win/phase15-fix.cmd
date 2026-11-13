@echo off
chcp 65001 >nul
title Oxide - Phase 15 fix
cd /d "%~dp0..\.."
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0phase15-fix.ps1"
echo.
call "%~dp0dev.cmd"
