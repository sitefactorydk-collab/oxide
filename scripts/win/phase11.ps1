# Phase 11: stop any running dev app, run all checks, check the production build, and commit
# when everything is green. Also makes a messy sample folder for the exit check:
# Documents\Oxide sample folder (made again only when it's missing).
# Logs: logs\check*.log, logs\phase11-build*.log, logs\commit*.log. The .cmd wrapper then
# starts the app.
. "$PSScriptRoot\_common.ps1"
Get-Process -Name oxide, oxide-mcp-mock -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Get-NetTCPConnection -LocalPort 1420 -State Listen -ErrorAction SilentlyContinue |
  ForEach-Object { Stop-Process -Id $_.OwningProcess -Force -ErrorAction SilentlyContinue }

# Sample folder for "organize this folder by file type and write a summary".
$sample = Join-Path ([Environment]::GetFolderPath('MyDocuments')) 'Oxide sample folder'
if (-not (Test-Path $sample)) {
  New-Item -ItemType Directory -Force -Path $sample | Out-Null
  Set-Content -Encoding utf8 -Path (Join-Path $sample 'shopping list.txt') -Value "Milk`r`nCoffee`r`nRye bread"
  Set-Content -Encoding utf8 -Path (Join-Path $sample 'meeting notes 2026-11-03.md') -Value "# Weekly sync`r`n- Ship Phase 11`r`n- Test the session panel"
  Set-Content -Encoding utf8 -Path (Join-Path $sample 'ideas.md') -Value "# Ideas`r`n- A dark theme for the tray menu"
  Set-Content -Encoding utf8 -Path (Join-Path $sample 'budget.csv') -Value "item,amount`r`nRent,9500`r`nFood,3200`r`nInternet,299"
  Set-Content -Encoding utf8 -Path (Join-Path $sample 'contacts.csv') -Value "name,city`r`nMikkel,Copenhagen`r`nAda,London"
  Set-Content -Encoding utf8 -Path (Join-Path $sample 'settings.json') -Value '{ "theme": "dark", "fontSize": 14 }'
  Set-Content -Encoding utf8 -Path (Join-Path $sample 'server.log') -Value "2026-11-07 10:00 started`r`n2026-11-07 10:05 stopped"
  Set-Content -Encoding utf8 -Path (Join-Path $sample 'script.py') -Value "print('hello from the sample folder')"
  foreach ($icon in '32x32.png', '128x128.png', 'icon.ico') {
    $src = Join-Path $Root "src-tauri\icons\$icon"
    if (Test-Path $src) { Copy-Item $src (Join-Path $sample "picture-$icon") }
  }
  Add-Content -Encoding utf8 -Path (Join-Path $Root 'logs\phase11-sample.log') -Value "made $sample"
}

& "$PSScriptRoot\check.ps1"
$checks = $LASTEXITCODE

Start-Log 'phase11-build'
Run 'vite build' 'npx vite build --logLevel warn'
End-Log
$build = $script:Failures

$msg = Join-Path $Root 'logs\commit-msg-phase11.txt'
if ($checks -eq 0 -and $build -eq 0 -and (Test-Path $msg)) {
  Copy-Item -Force $msg (Join-Path $Root 'logs\commit-msg.txt')
  & "$PSScriptRoot\commit.ps1"
} else {
  Write-Host 'Something failed: not committing. See logs\check-summary.log and logs\phase11-build-summary.log' -ForegroundColor Red
}
