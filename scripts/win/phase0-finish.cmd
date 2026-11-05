@echo off
chcp 65001 >nul
title Oxide - Phase 0 finish
cd /d "%~dp0..\.."
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0phase0-finish.ps1"
echo.
call "%~dp0dev.cmd"
