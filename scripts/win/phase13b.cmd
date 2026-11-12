@echo off
chcp 65001 >nul
title Oxide - Phase 13b
cd /d "%~dp0..\.."
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0phase13b.ps1"
echo.
call "%~dp0dev.cmd"
