@echo off
chcp 65001 >nul
title Oxide - Music test and extensions test diagnosis
cd /d "%~dp0..\.."
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0ext-diag2.ps1"
echo.
pause
