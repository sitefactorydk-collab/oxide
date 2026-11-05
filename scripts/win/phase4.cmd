@echo off
chcp 65001 >nul
title Oxide - Phase 4
cd /d "%~dp0..\.."
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0phase4.ps1"
echo.
call "%~dp0dev.cmd"
