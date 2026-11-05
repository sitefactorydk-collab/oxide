@echo off
chcp 65001 >nul
title Oxide - Phase 4 fixes
cd /d "%~dp0..\.."
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0phase4-fix.ps1"
echo.
call "%~dp0dev.cmd"
