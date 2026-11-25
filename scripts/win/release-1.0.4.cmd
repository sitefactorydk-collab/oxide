@echo off
chcp 65001 >nul
title Oxide - release v1.0.4
cd /d "%~dp0..\.."
rem Runs in the console (not redirected), so Git Credential Manager can open its GitHub sign-in.
rem Run it after Phases 25 and 26 are committed (they are: 5121665 and before).
echo.
echo == 1. Commit the version bump (1.0.3 -^> 1.0.4)
git add src-tauri/tauri.conf.json package.json package-lock.json Cargo.toml Cargo.lock README.md PROGRESS.md scripts/win/release-1.0.4.cmd
git commit -F logs\commit-msg-release-1.0.4.txt
if errorlevel 1 goto failed
echo.
echo == 2. Push main (the 10 commits since v1.0.3 and the bump)
git push origin main
if errorlevel 1 goto failed
echo.
echo == 3. Tag v1.0.4 and push the tag (starts the Release workflow on GitHub)
git tag -a v1.0.4 -m "Oxide v1.0.4"
if errorlevel 1 goto failed
git push origin v1.0.4
if errorlevel 1 goto failed
echo.
echo Done. Watch it build: https://github.com/sitefactorydk-collab/oxide/actions
echo The draft release appears at: https://github.com/sitefactorydk-collab/oxide/releases
echo Release notes to paste into it: logs\release-notes-1.0.4.md
goto end
:failed
echo.
echo Something failed (exit code %errorlevel%). Nothing after that step ran.
:end
git status -sb | findstr /b "##"
pause
