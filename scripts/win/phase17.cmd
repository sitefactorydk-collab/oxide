@echo off
chcp 65001 >nul
title Oxide - Phase 17
cd /d "%~dp0..\.."
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0phase17.ps1"
echo.
echo Installers: target\release\bundle (nsis and msi). See docs\RELEASING.md.
pause
