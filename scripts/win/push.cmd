@echo off
chcp 65001 >nul
title Oxide - git push
cd /d "%~dp0..\.."
rem Runs in the console (not redirected), so Git Credential Manager can open its GitHub sign-in.
git config --get credential.helper
git push origin main
echo.
echo Exit code: %errorlevel%
git status -sb | findstr /b "##"
pause
