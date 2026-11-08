@echo off
chcp 65001 >nul
title Oxide - Phase 12
cd /d "%~dp0..\.."
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0phase12.ps1"
echo.
call "%~dp0dev.cmd"
