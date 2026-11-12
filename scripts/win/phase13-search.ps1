# Phase 13 follow-up (Google search): the browser agent's `search` tool, searching instead of
# guessing when an address doesn't exist, and hints after failed fetches. Same steps as
# phase13.ps1, and the real-browser test also searches Google for real. Then the exit check against a real browser: Python, the browser packages and (only
# when no Chrome or Edge is installed) Chromium are installed into target\browser-test-python
# (kept between runs), and the agent opens, reads, clicks and screenshots a local web site.
# When Strata (127.0.0.1:47831) or LM Studio (127.0.0.1:1234) answers, Browser-Use also runs
# a whole task with its first model. Commits when everything is green.
# Logs: logs\phase13s-prep*.log, logs\check*.log, logs\phase13s-build*.log,
# logs\phase13s-exit*.log, logs\commit*.log. The .cmd wrapper then starts the app.
. "$PSScriptRoot\_common.ps1"
Get-Process -Name oxide, oxide-mcp-mock -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Get-NetTCPConnection -LocalPort 1420 -State Listen -ErrorAction SilentlyContinue |
  ForEach-Object { Stop-Process -Id $_.OwningProcess -Force -ErrorAction SilentlyContinue }

$triple = (rustc -vV | Select-String '^host: (.+)$').Matches[0].Groups[1].Value.Trim()
$uvRel = "src-tauri\binaries\uv-$triple.exe"
$uvAbs = Join-Path $Root $uvRel

Start-Log 'phase13s-prep'
Run 'fetch uv' 'node scripts\fetch-uv.mjs'
if ((Test-Path $uvAbs) -and ((Get-Item $uvAbs).Length -gt 0)) {
  Run 'uv lock' "$uvRel lock --project python --python 3.12"
} else {
  Add-Content -Encoding utf8 -Path $script:SummaryFile -Value 'uv lock                      FAIL  (no uv sidecar)'
  $script:Failures++
}
Run 'bindings' 'npm run bindings'
End-Log
$prep = $script:Failures

& "$PSScriptRoot\check.ps1"
$checks = $LASTEXITCODE

Start-Log 'phase13s-build'
Run 'vite build' 'npx vite build --logLevel warn'
End-Log
$build = $script:Failures

# A local model for Browser-Use, when one runs: Strata first, then LM Studio.
function Find-LocalModel {
  foreach ($c in @(
      @{ kind = 'strata'; url = 'http://127.0.0.1:47831/v1' },
      @{ kind = 'lmstudio'; url = 'http://127.0.0.1:1234/v1' })) {
    try {
      $r = Invoke-RestMethod -Uri "$($c.url)/models" -TimeoutSec 5
      $id = @($r.data)[0].id
      if ($id) { return @{ kind = $c.kind; url = $c.url; model = $id } }
    } catch { }
  }
  return $null
}

Start-Log 'phase13s-exit'
$env:OXIDE_UV = $uvAbs
$env:OXIDE_TEST_PYTHON_ROOT = Join-Path $Root 'target\browser-test-python'
$env:OXIDE_TEST_REAL_SEARCH = '1'
$llm = Find-LocalModel
if ($llm) {
  $env:OXIDE_TEST_LLM_KIND = $llm.kind
  $env:OXIDE_TEST_LLM_BASE_URL = $llm.url
  $env:OXIDE_TEST_LLM_MODEL = $llm.model
  Add-Content -Encoding utf8 -Path $script:LogFile -Value "browser_task model: $($llm.kind) $($llm.model)"
} else {
  Add-Content -Encoding utf8 -Path $script:SummaryFile -Value 'browser_task                 SKIPPED  (neither Strata nor LM Studio answered; start one and run this again to include it)'
}
Run 'real browser' 'cargo test -p oxide-pyenv --test real_browser -- --ignored --nocapture'

# The agent's own unit tests, in the environment the real test just installed.
$py = Join-Path $env:OXIDE_TEST_PYTHON_ROOT 'python\venv\Scripts\python.exe'
$chrome = @(
  "$env:ProgramFiles\Google\Chrome\Application\chrome.exe",
  "${env:ProgramFiles(x86)}\Google\Chrome\Application\chrome.exe",
  "$env:LOCALAPPDATA\Google\Chrome\Application\chrome.exe",
  "${env:ProgramFiles(x86)}\Microsoft\Edge\Application\msedge.exe",
  "$env:ProgramFiles\Microsoft\Edge\Application\msedge.exe"
) | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
if ((Test-Path $py) -and $chrome) {
  $env:OXIDE_TEST_CHROME = $chrome
  Run 'agent unit tests' "`"$py`" -m unittest discover -s python\tests -v"
  Remove-Item Env:\OXIDE_TEST_CHROME -ErrorAction SilentlyContinue
} else {
  Add-Content -Encoding utf8 -Path $script:SummaryFile -Value 'agent unit tests             SKIPPED  (no environment or no Chrome/Edge)'
}
foreach ($v in 'OXIDE_TEST_REAL_SEARCH', 'OXIDE_UV', 'OXIDE_TEST_PYTHON_ROOT', 'OXIDE_TEST_LLM_KIND', 'OXIDE_TEST_LLM_BASE_URL', 'OXIDE_TEST_LLM_MODEL') {
  Remove-Item "Env:\$v" -ErrorAction SilentlyContinue
}
End-Log
$exit = $script:Failures

$msg = Join-Path $Root 'logs\commit-msg-phase13-search.txt'
if ($prep -eq 0 -and $checks -eq 0 -and $build -eq 0 -and $exit -eq 0 -and (Test-Path $msg)) {
  Copy-Item -Force $msg (Join-Path $Root 'logs\commit-msg.txt')
  & "$PSScriptRoot\commit.ps1"
} else {
  Write-Host 'Something failed: not committing. See logs\phase13s-prep-summary.log, check-summary.log, phase13s-build-summary.log and phase13s-exit-summary.log' -ForegroundColor Red
}
