@echo off
chcp 65001 >nul
title Oxide - Phase 1
cd /d "%~dp0..\.."
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0phase1.ps1"
echo.
call "%~dp0dev.cmd"
