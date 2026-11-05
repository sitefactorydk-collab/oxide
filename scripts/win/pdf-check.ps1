# Diagnostics for "the model wouldn't translate my whole PDF": what text Oxide extracted from
# the most recent PDFs, and Strata's output-length defaults. Output: logs\pdf-check.log
. "$PSScriptRoot\_common.ps1"
$log = Join-Path $Root 'logs\pdf-check.log'
Set-Content -Encoding utf8 -Path $log -Value "Oxide PDF check  $(Get-Date -Format s)"
function L([string]$s) { Add-Content -Encoding utf8 -Path $log -Value $s }

$dir = Join-Path $env:APPDATA 'dev.oxide.app\attachments'
L "attachments: $dir"
$pdfs = Get-ChildItem $dir -Filter *.pdf -ErrorAction SilentlyContinue | Sort-Object LastWriteTime -Descending | Select-Object -First 3
foreach ($p in $pdfs) {
  L ""
  L "== $($p.Name)  $([math]::Round($p.Length / 1KB)) KB  $($p.LastWriteTime)"
  $side = "$($p.FullName).txt"
  if (-not (Test-Path $side)) { L '  no extracted text (sidecar missing)'; continue }
  $t = Get-Content -Raw -Encoding utf8 $side
  if ($null -eq $t) { $t = '' }
  $pages = ([regex]::Matches($t, '\[Page \d+\]')).Count
  $lines = ($t -split "`n").Count
  $words = ($t -split '\s+' | Where-Object { $_ }).Count
  $blank = ($t -split "`n" | Where-Object { $_.Trim() -eq '' }).Count
  $short = ($t -split "`n" | Where-Object { $_.Trim().Length -gt 0 -and $_.Trim().Length -le 2 }).Count
  L "  chars=$($t.Length)  ~tokens=$([math]::Round($t.Length / 4))  pages=$pages  lines=$lines  blank lines=$blank  1-2 char lines=$short  words=$words"
  $sample = $t.Substring(0, [math]::Min(400, $t.Length)) -replace "`r", '' -replace "`n", ' / '
  L "  first 400 chars: $sample"
}
if (-not $pdfs) { L 'no PDFs found' }

L ""
L "== Strata"
foreach ($u in 'http://127.0.0.1:47831/props', 'http://127.0.0.1:47831/v1/props', 'http://127.0.0.1:47831/v1/models') {
  try {
    $r = Invoke-WebRequest -Uri $u -TimeoutSec 5 -UseBasicParsing
    $c = $r.Content
    L "$u -> $($r.StatusCode): $($c.Substring(0, [math]::Min(1500, $c.Length)))"
  } catch { L "$u -> $($_.Exception.Message)" }
}
Write-Host "Wrote $log"
