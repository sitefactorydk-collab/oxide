# Phase 14: the real extension in Edge or Chrome on Windows, paired through its popup and
# driven through every tool (crates\oxide-bridge\tests\real_chrome.rs). Needs Phase 13's test
# Python (Playwright). Logs: logs\phase14-chrome*.log. Commits nothing.
. "$PSScriptRoot\_common.ps1"
Start-Log 'phase14-chrome'
Run 'extension' 'npm run extension'
$py = Get-ChildItem -Path (Join-Path $Root 'target\browser-test-python') -Recurse -Filter pyvenv.cfg -ErrorAction SilentlyContinue |
  ForEach-Object { Get-Item -ErrorAction SilentlyContinue (Join-Path $_.DirectoryName 'Scripts\python.exe') } |
  Select-Object -First 1
$browsers = @(
  "${env:ProgramFiles(x86)}\Microsoft\Edge\Application\msedge.exe",
  "$env:ProgramFiles\Google\Chrome\Application\chrome.exe"
) | Where-Object { Test-Path $_ }
if ($py -and $browsers.Count -gt 0) {
  $env:OXIDE_TEST_PYTHON = $py.FullName
  $env:OXIDE_TEST_CHROME = $browsers[0]
  Add-Content -Encoding utf8 -Path $script:LogFile -Value "python: $($py.FullName)`nbrowser: $($browsers[0])"
  Run 'real Chrome' 'cargo test -p oxide-bridge --test real_chrome -- --ignored --nocapture'
} else {
  Add-Content -Encoding utf8 -Path $script:SummaryFile -Value 'SKIP: no test Python with Playwright, or no browser'
}
End-Log
