@echo off
chcp 65001 >nul
title Oxide - Phase 3
cd /d "%~dp0..\.."
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0phase3.ps1"
echo.
call "%~dp0dev.cmd"
