@echo off
title Oxide - Phase 0 setup and checks
cd /d "%~dp0..\.."
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0phase0.ps1"
echo.
echo Summary: logs\phase0-summary.log
call "%~dp0dev.cmd"
