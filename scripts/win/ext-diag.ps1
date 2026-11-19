# Diagnoses the flaky oxide-mcp test install_configure_toggle_update_uninstall (os error 32).
# Runs only that test 3 times while a background job records, once a second, every running
# oxide-mcp-mock process (pid, start time, command line) and any other process whose path is
# under the temp folder. Logs: logs\ext-diag.log (test output), logs\ext-diag-procs.log.
. "$PSScriptRoot\_common.ps1"
$procLog = Join-Path $Root 'logs\ext-diag-procs.log'
Set-Content -Encoding utf8 -Path $procLog -Value "process samples $(Get-Date -Format s)"
$poller = Start-Job -ArgumentList $procLog, $env:TEMP -ScriptBlock {
  param($out, $temp)
  while ($true) {
    $t = Get-Date -Format 'HH:mm:ss.fff'
    $ps = Get-CimInstance Win32_Process | Where-Object {
      $_.Name -like 'oxide-mcp-mock*' -or ($_.ExecutablePath -and $_.ExecutablePath -like "$temp*")
    }
    foreach ($p in $ps) {
      Add-Content -Encoding utf8 -Path $out -Value "$t pid=$($p.ProcessId) parent=$($p.ParentProcessId) name=$($p.Name) started=$($p.CreationDate) cmd=$($p.CommandLine)"
    }
    if (-not $ps) { Add-Content -Encoding utf8 -Path $out -Value "$t (none)" }
    Start-Sleep -Milliseconds 1000
  }
}
Start-Log 'ext-diag'
foreach ($i in 1..3) {
  Run "extensions test, run $i" 'cargo test -p oxide-mcp --test extensions install_configure_toggle_update_uninstall -- --nocapture'
}
End-Log
Stop-Job $poller; Remove-Job $poller
Write-Host 'Done. Logs: logs\ext-diag-summary.log, logs\ext-diag.log, logs\ext-diag-procs.log' -ForegroundColor Green
