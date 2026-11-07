@echo off
chcp 65001 >nul
title Oxide - Phase 9
cd /d "%~dp0..\.."
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0phase9.ps1"
echo.
call "%~dp0dev.cmd"
