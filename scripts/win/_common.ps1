# Shared helpers for Oxide's Windows dev scripts.
$ErrorActionPreference = 'Continue'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
$OutputEncoding = [Text.Encoding]::UTF8
$env:NO_COLOR = '1'          # plain logs; cargo/vite colours are useless in log files
$env:CARGO_TERM_COLOR = 'never'

$Root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
Set-Location $Root
New-Item -ItemType Directory -Force -Path (Join-Path $Root 'logs') | Out-Null

# Per-user cargo/rustup may be missing from PATH in a fresh shell.
$cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
if ((Test-Path $cargoBin) -and ($env:Path -notlike "*$cargoBin*")) { $env:Path = "$cargoBin;$env:Path" }

# Used by check-toolchain.ps1
function Step([string]$Name, [scriptblock]$Body) {
  Write-Output ""
  Write-Output "=== $Name ==="
  $sw = [Diagnostics.Stopwatch]::StartNew()
  try { & $Body 2>&1 | ForEach-Object { "$_" } } catch { Write-Output "ERROR: $_" }
  Write-Output "--- exit=$LASTEXITCODE ($([int]$sw.Elapsed.TotalSeconds)s)"
}

# Start a log pair: <name>.log (full output) and <name>-summary.log (PASS/FAIL table).
function Start-Log([string]$Name) {
  $script:LogFile = Join-Path $Root "logs\$Name.log"
  $script:SummaryFile = Join-Path $Root "logs\$Name-summary.log"
  $script:Failures = 0
  $header = "Oxide $Name  $(Get-Date -Format s)"
  Set-Content -Encoding utf8 -Path $script:LogFile -Value $header
  Set-Content -Encoding utf8 -Path $script:SummaryFile -Value $header
}

# Run a command through cmd.exe (so stderr is plain text, not PowerShell error records),
# echo it to the console and append it to the log. Records PASS/FAIL in the summary.
function Run([string]$Name, [string]$Cmd) {
  Write-Host ">> $Name" -ForegroundColor Cyan
  Add-Content -Encoding utf8 -Path $script:LogFile -Value "`n=== $Name ===`n> $Cmd"
  $sw = [Diagnostics.Stopwatch]::StartNew()
  cmd /c "$Cmd 2>&1" | ForEach-Object {
    Write-Host $_
    Add-Content -Encoding utf8 -Path $script:LogFile -Value $_
  }
  $code = $LASTEXITCODE
  $status = if ($code -eq 0) { 'PASS' } else { 'FAIL'; $script:Failures++ }
  $line = '{0,-28} {1}  exit={2}  {3}s' -f $Name, $status, $code, [int]$sw.Elapsed.TotalSeconds
  Add-Content -Encoding utf8 -Path $script:SummaryFile -Value $line
  Add-Content -Encoding utf8 -Path $script:LogFile -Value "--- $line"
  Write-Host "   $status" -ForegroundColor ($(if ($code -eq 0) { 'Green' } else { 'Red' }))
}

function End-Log {
  $result = if ($script:Failures -eq 0) { 'ALL PASSED' } else { "$($script:Failures) FAILED" }
  Add-Content -Encoding utf8 -Path $script:SummaryFile -Value "`nRESULT: $result  $(Get-Date -Format s)"
  Write-Host "`nRESULT: $result  (logs\$(Split-Path $script:SummaryFile -Leaf))" -ForegroundColor ($(if ($script:Failures -eq 0) { 'Green' } else { 'Red' }))
}
