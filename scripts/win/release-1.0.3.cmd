@echo off
chcp 65001 >nul
title Oxide - release v1.0.3
cd /d "%~dp0..\.."
rem Runs in the console (not redirected), so Git Credential Manager can open its GitHub sign-in.
rem Run it after scripts\win\fix-ansi.cmd has committed and pushed the colour-code fix, and CI is green.
echo == 0. Check that the colour-code fix is committed
git diff --quiet HEAD -- crates/oxide-agent/src/tools/shell.rs
if errorlevel 1 (
  echo The fix in crates\oxide-agent\src\tools\shell.rs is not committed yet. Run scripts\win\fix-ansi.cmd first.
  goto end
)
echo.
echo == 1. Commit the version bump (1.0.2 -^> 1.0.3)
git add src-tauri/tauri.conf.json package.json package-lock.json Cargo.toml Cargo.lock scripts/win/release-1.0.3.cmd
git commit -F logs\commit-msg-release-1.0.3.txt
if errorlevel 1 goto failed
echo.
echo == 2. Push main
git push origin main
if errorlevel 1 goto failed
echo.
echo == 3. Tag v1.0.3 and push the tag (starts the Release workflow on GitHub)
git tag -a v1.0.3 -m "Oxide v1.0.3"
if errorlevel 1 goto failed
git push origin v1.0.3
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
