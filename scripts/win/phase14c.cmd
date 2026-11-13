@echo off
chcp 65001 >nul
title Oxide - Phase 14c
cd /d "%~dp0..\.."
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0phase14c.ps1"
echo.
call "%~dp0dev.cmd"
