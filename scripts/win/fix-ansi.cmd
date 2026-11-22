@echo off
chcp 65001 >nul
title Oxide - colour codes in PowerShell errors
cd /d "%~dp0..\.."
rem Runs in the console, so Git Credential Manager can open its GitHub sign-in for the push.
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0fix-ansi.ps1"
echo.
echo Logs: logs\check-summary.log, logs\commit-summary.log.
pause
