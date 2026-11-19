@echo off
chcp 65001 >nul
title Oxide - extensions test diagnosis
cd /d "%~dp0..\.."
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0ext-diag.ps1"
echo.
pause
