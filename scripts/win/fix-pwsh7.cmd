@echo off
chcp 65001 >nul
title Oxide - PowerShell 7 error records
cd /d "%~dp0..\.."
rem Runs in the console, so Git Credential Manager can open its GitHub sign-in for the push.
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0fix-pwsh7.ps1"
echo.
echo Logs: logs\check-summary.log, logs\commit-summary.log.
pause
