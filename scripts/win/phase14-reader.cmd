@echo off
chcp 65001 >nul
title Oxide - Phase 14 reader fix
cd /d "%~dp0..\.."
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0phase14-reader.ps1"
echo.
call "%~dp0dev.cmd"
