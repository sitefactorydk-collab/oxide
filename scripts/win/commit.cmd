@echo off
chcp 65001 >nul
title Oxide - commit
cd /d "%~dp0..\.."
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0commit.ps1"
echo.
pause
