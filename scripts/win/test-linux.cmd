@echo off
chcp 65001 >nul
title Oxide - Linux package test (Docker)
cd /d "%~dp0..\.."
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0test-linux.ps1"
echo.
pause
