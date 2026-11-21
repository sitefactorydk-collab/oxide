@echo off
chcp 65001 >nul
title Oxide - release v0.1.3
cd /d "%~dp0..\.."
rem Runs in the console (not redirected), so Git Credential Manager can open its GitHub sign-in.
echo == 1. Commit the version bump (0.1.2 -^> 0.1.3)
git add src-tauri/tauri.conf.json package.json package-lock.json Cargo.toml Cargo.lock scripts/win/release-0.1.3.cmd
git commit -F logs\commit-msg-release-0.1.3.txt
if errorlevel 1 goto failed
echo.
echo == 2. Push main
git push origin main
if errorlevel 1 goto failed
echo.
echo == 3. Tag v0.1.3 and push the tag (starts the Release workflow on GitHub)
git tag -a v0.1.3 -m "Oxide v0.1.3"
if errorlevel 1 goto failed
git push origin v0.1.3
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
