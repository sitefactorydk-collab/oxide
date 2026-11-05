# Sends the same small prompt to each local endpoint with different thinking switches and logs
# whether the reply carried reasoning, how long it was and how long it took.
# Output: logs\local-thinking.log (read by the agent).
. "$PSScriptRoot\_common.ps1"
$log = Join-Path $Root 'logs\local-thinking.log'
Set-Content -Encoding utf8 -Path $log -Value "Oxide local thinking test  $(Get-Date -Format s)"
function L([string]$s) { Write-Host $s; Add-Content -Encoding utf8 -Path $log -Value $s }

$endpoints = @(
  @{ Name = 'strata'; Base = 'http://127.0.0.1:47831/v1' },
  @{ Name = 'lmstudio'; Base = 'http://127.0.0.1:1234/v1' }
)
$prompt = 'What is 17 + 25? Reply with just the number.'
$variants = [ordered]@{
  'baseline'                    = @{}
  'reasoning_effort=low'        = @{ reasoning_effort = 'low' }
  'reasoning_effort=high'       = @{ reasoning_effort = 'high' }
  'reasoning_effort=none'       = @{ reasoning_effort = 'none' }
  'enable_thinking=false'       = @{ chat_template_kwargs = @{ enable_thinking = $false } }
  'reasoning={effort:low}'      = @{ reasoning = @{ effort = 'low' } }
  'thinking_budget_tokens=64'   = @{ thinking_budget_tokens = 64 }
  'prompt /no_think'            = @{ __suffix = ' /no_think' }
}

foreach ($ep in $endpoints) {
  L ""
  L "=== $($ep.Name)  $($ep.Base)"
  try {
    $models = Invoke-RestMethod -Uri "$($ep.Base)/models" -TimeoutSec 5
  } catch { L "  not reachable: $($_.Exception.Message)"; continue }
  $ids = @($models.data | ForEach-Object { $_.id } | Where-Object { $_ -notmatch 'embed' })
  L "  models: $($ids -join ', ')"
  if ($ep.Name -eq 'lmstudio') {
    try {
      $loaded = Invoke-RestMethod -Uri ($ep.Base -replace '/v1$', '/api/v0/models') -TimeoutSec 5
      $l = @($loaded.data | Where-Object { $_.state -eq 'loaded' -and $_.type -ne 'embeddings' } | ForEach-Object { $_.id })
      if ($l.Count -gt 0) { $ids = $l; L "  loaded: $($l -join ', ')" }
    } catch {}
  }
  $q = @($ids | Where-Object { $_ -match 'qwen' })
  $model = if ($q.Count -gt 0) { $q[0] } else { $ids[0] }
  if (-not $model) { L '  no chat model'; continue }
  L "  testing model: $model"
  foreach ($name in $variants.Keys) {
    $extra = $variants[$name]
    $text = $prompt
    if ($extra.ContainsKey('__suffix')) { $text = $prompt + $extra['__suffix'] }
    $body = @{ model = $model; stream = $false; max_tokens = 1500;
               messages = @(@{ role = 'user'; content = $text }) }
    foreach ($k in $extra.Keys) { if ($k -ne '__suffix') { $body[$k] = $extra[$k] } }
    $json = $body | ConvertTo-Json -Depth 8 -Compress
    $sw = [Diagnostics.Stopwatch]::StartNew()
    try {
      $r = Invoke-WebRequest -Uri "$($ep.Base)/chat/completions" -Method Post -ContentType 'application/json' `
        -Body ([Text.Encoding]::UTF8.GetBytes($json)) -TimeoutSec 240 -UseBasicParsing
      $sec = [math]::Round($sw.Elapsed.TotalSeconds, 1)
      $v = $r.Content | ConvertFrom-Json
      $m = $v.choices[0].message
      $reason = "$($m.reasoning_content)$($m.reasoning)"
      $content = "$($m.content)"
      $think = ([regex]::Match($content, '(?s)<think>(.*?)</think>')).Groups[1].Value
      L ("  {0,-26} 200  {1,5}s  reasoning={2,5} chars  <think>={3,5} chars  completion_tokens={4}  answer={5}" -f `
        $name, $sec, $reason.Length, $think.Length, $v.usage.completion_tokens,
        (($content -replace '(?s)<think>.*?</think>', '').Trim() -replace '\s+', ' ').Substring(0, [math]::Min(40, (($content -replace '(?s)<think>.*?</think>', '').Trim() -replace '\s+', ' ').Length)))
    } catch {
      $sec = [math]::Round($sw.Elapsed.TotalSeconds, 1)
      $code = try { [int]$_.Exception.Response.StatusCode } catch { 0 }
      $detail = try { $_.ErrorDetails.Message } catch { '' }
      if (-not $detail) { $detail = $_.Exception.Message }
      L ("  {0,-26} ERR {1}  {2,5}s  {3}" -f $name, $code, $sec, ($detail -replace '\s+', ' ').Substring(0, [math]::Min(200, ($detail -replace '\s+', ' ').Length)))
    }
  }
}
L ""
L "done $(Get-Date -Format s)"
