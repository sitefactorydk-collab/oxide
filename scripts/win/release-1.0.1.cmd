@echo off
chcp 65001 >nul
title Oxide - release v1.0.1
cd /d "%~dp0..\.."
rem Runs in the console (not redirected), so Git Credential Manager can open its GitHub sign-in.
echo == 1. Commit the README and the version bump (0.1.3 -^> 1.0.1)
git add README.md src-tauri/tauri.conf.json package.json package-lock.json Cargo.toml Cargo.lock scripts/win/release-1.0.1.cmd
git commit -F logs\commit-msg-release-1.0.1.txt
if errorlevel 1 goto failed
echo.
echo == 2. Push main
git push origin main
if errorlevel 1 goto failed
echo.
echo == 3. Tag v1.0.1 and push the tag (starts the Release workflow on GitHub)
git tag -a v1.0.1 -m "Oxide v1.0.1"
if errorlevel 1 goto failed
git push origin v1.0.1
if errorlevel 1 goto failed
echo.
echo Done. Watch it build: https://github.com/sitefactorydk-collab/oxide/actions
echo The draft release appears at: https://github.com/sitefactorydk-collab/oxide/releases
goto end
:failed
echo.
echo Something failed (exit code %errorlevel%). Nothing after that step ran.
:end
git status -sb | findstr /b "##"
pause
