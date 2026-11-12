# Phase 14: Chrome extension & WebSocket bridge. Regenerates the bindings, builds the
# extension (extension\dist, the folder to load unpacked), runs every check (fmt, prettier,
# clippy, cargo test, typecheck, lint, Vitest) and vite build, then commits when all of that
# is green. Afterwards, when Phase 13's test Python (with Playwright) is there, it also tries
# the real-Chrome test (informational: Chrome may refuse --load-extension; never blocks).
# Logs: logs\phase14-prep*.log, logs\check*.log, logs\phase14-build*.log,
# logs\phase14-chrome*.log, logs\commit*.log. The .cmd wrapper then starts the app.
. "$PSScriptRoot\_common.ps1"
Get-Process -Name oxide, oxide-mcp-mock -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Get-NetTCPConnection -LocalPort 1420 -State Listen -ErrorAction SilentlyContinue |
  ForEach-Object { Stop-Process -Id $_.OwningProcess -Force -ErrorAction SilentlyContinue }

Start-Log 'phase14-prep'
Run 'bindings' 'npm run bindings'
Run 'extension' 'npm run extension'
End-Log
$prep = $script:Failures

& "$PSScriptRoot\check.ps1"
$checks = $LASTEXITCODE

Start-Log 'phase14-build'
Run 'vite build' 'npx vite build --logLevel warn'
End-Log
$build = $script:Failures

$msg = Join-Path $Root 'logs\commit-msg-phase14.txt'
if ($prep -eq 0 -and $checks -eq 0 -and $build -eq 0 -and (Test-Path $msg)) {
  Copy-Item -Force $msg (Join-Path $Root 'logs\commit-msg.txt')
  & "$PSScriptRoot\commit.ps1"
} else {
  Write-Host 'Something failed: not committing. See logs\phase14-prep-summary.log, check-summary.log and phase14-build-summary.log' -ForegroundColor Red
}

# Informational: the real extension in a real browser, driven like a user (pairing through
# its popup). Needs Python with Playwright (Phase 13's test environment) and a browser that
# loads unpacked extensions from the command line.
Start-Log 'phase14-chrome'
# The test environment's venv (its pyvenv.cfg sits next to Scripts\python.exe).
$py = Get-ChildItem -Path (Join-Path $Root 'target\browser-test-python') -Recurse -Filter pyvenv.cfg -ErrorAction SilentlyContinue |
  ForEach-Object { Get-Item -ErrorAction SilentlyContinue (Join-Path $_.DirectoryName 'Scripts\python.exe') } |
  Select-Object -First 1
$browsers = @(
  (Get-ChildItem -Path (Join-Path $Root 'target\browser-test-python') -Recurse -Filter chrome.exe -ErrorAction SilentlyContinue | Select-Object -First 1 | ForEach-Object { $_.FullName }),
  "${env:ProgramFiles(x86)}\Microsoft\Edge\Application\msedge.exe",
  "$env:ProgramFiles\Google\Chrome\Application\chrome.exe"
) | Where-Object { $_ -and (Test-Path $_) }
if ($py -and $browsers.Count -gt 0) {
  $env:OXIDE_TEST_PYTHON = $py.FullName
  $env:OXIDE_TEST_CHROME = $browsers[0]
  Add-Content -Encoding utf8 -Path $script:LogFile -Value "python: $($py.FullName)`nbrowser: $($browsers[0])"
  Run 'real Chrome (informational)' 'cargo test -p oxide-bridge --test real_chrome -- --ignored --nocapture'
} else {
  Add-Content -Encoding utf8 -Path $script:SummaryFile -Value 'SKIP real Chrome: no test Python with Playwright, or no browser'
}
End-Log
