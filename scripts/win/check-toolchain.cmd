@echo off
title Oxide - toolchain check
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0check-toolchain.ps1"
echo Toolchain check finished. Log: logs\toolchain.log
timeout /t 3 >nul
