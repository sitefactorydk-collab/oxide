@echo off
chcp 65001 >nul
title Oxide - Phase 12b
cd /d "%~dp0..\.."
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0phase12b.ps1"
echo.
call "%~dp0dev.cmd"
