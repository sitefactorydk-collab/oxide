@echo off
chcp 65001 >nul
title Oxide - Phase 13 search
cd /d "%~dp0..\.."
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0phase13-search.ps1"
echo.
call "%~dp0dev.cmd"
