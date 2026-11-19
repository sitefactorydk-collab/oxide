# Phase 19: rechecks the fixed Music test, then diagnoses the flaky oxide-mcp test
# install_configure_toggle_update_uninstall (os error 32 while an update renames the folder).
#  1. Vitest: src/features/generation/Generation.test.tsx only.
#  2. The whole extensions test binary 4 times (alone, the test passes; it fails beside the
#     others). A background job samples every 250 ms:
#       - every oxide-mcp-mock process (pid, parent, start time);
#       - which processes hold files in the test's extension folder
#         (%TEMP%\.tmp*\extensions\mock-ext), asked from Windows' Restart Manager.
# Nothing is committed. Logs: logs\ext-diag2-summary.log, logs\ext-diag2.log,
# logs\ext-diag2-procs.log.
. "$PSScriptRoot\_common.ps1"
$procLog = Join-Path $Root 'logs\ext-diag2-procs.log'
Set-Content -Encoding utf8 -Path $procLog -Value "process samples $(Get-Date -Format s)"

$poller = Start-Job -ArgumentList $procLog, $env:TEMP -ScriptBlock {
  param($out, $temp)
  Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public static class RmHolders {
  [StructLayout(LayoutKind.Sequential)]
  struct RM_UNIQUE_PROCESS { public int dwProcessId; public System.Runtime.InteropServices.ComTypes.FILETIME ProcessStartTime; }
  [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
  struct RM_PROCESS_INFO {
    public RM_UNIQUE_PROCESS Process;
    [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 256)] public string strAppName;
    [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 64)] public string strServiceShortName;
    public int ApplicationType; public uint AppStatus; public uint TSSessionId;
    [MarshalAs(UnmanagedType.Bool)] public bool bRestartable;
  }
  [DllImport("rstrtmgr.dll", CharSet = CharSet.Unicode)] static extern int RmStartSession(out uint h, int f, string key);
  [DllImport("rstrtmgr.dll")] static extern int RmEndSession(uint h);
  [DllImport("rstrtmgr.dll", CharSet = CharSet.Unicode)] static extern int RmRegisterResources(uint h, uint n, string[] files, uint a, IntPtr b, uint c, string[] d);
  [DllImport("rstrtmgr.dll")] static extern int RmGetList(uint h, out uint needed, ref uint n, [In, Out] RM_PROCESS_INFO[] info, ref uint reasons);
  public static List<string> Who(string[] files) {
    var result = new List<string>();
    uint h; if (RmStartSession(out h, 0, Guid.NewGuid().ToString()) != 0) return result;
    try {
      if (RmRegisterResources(h, (uint)files.Length, files, 0, IntPtr.Zero, 0, null) != 0) return result;
      uint needed = 0, n = 0, reasons = 0;
      int r = RmGetList(h, out needed, ref n, null, ref reasons);
      if (r == 234 && needed > 0) {
        var info = new RM_PROCESS_INFO[needed]; n = needed;
        if (RmGetList(h, out needed, ref n, info, ref reasons) == 0)
          for (int i = 0; i < n; i++) result.Add(info[i].Process.dwProcessId + " " + info[i].strAppName + " " + info[i].strServiceShortName);
      }
    } finally { RmEndSession(h); }
    return result;
  }
}
'@
  while ($true) {
    $t = Get-Date -Format 'HH:mm:ss.fff'
    $lines = @()
    foreach ($p in (Get-CimInstance Win32_Process -Filter "Name like 'oxide-mcp-mock%'")) {
      $lines += "$t mock pid=$($p.ProcessId) parent=$($p.ParentProcessId) started=$($p.CreationDate)"
    }
    $dirs = Get-ChildItem -Path $temp -Directory -Filter '.tmp*' -ErrorAction SilentlyContinue |
      ForEach-Object { Join-Path $_.FullName 'extensions\mock-ext' } |
      Where-Object { Test-Path $_ }
    foreach ($d in $dirs) {
      $files = @(Get-ChildItem -Path $d -Recurse -File -ErrorAction SilentlyContinue | ForEach-Object FullName)
      if ($files.Count -gt 0) {
        foreach ($w in [RmHolders]::Who($files)) { $lines += "$t holder of $d : $w" }
      }
    }
    if ($lines.Count -eq 0) { $lines = @("$t (none)") }
    Add-Content -Encoding utf8 -Path $procLog -Value $lines
    Start-Sleep -Milliseconds 250
  }
}

Start-Log 'ext-diag2'
Run 'Vitest Generation.test.tsx' 'npx vitest run src/features/generation/Generation.test.tsx'
foreach ($i in 1..4) {
  Run "extensions binary, run $i" 'cargo test -p oxide-mcp --test extensions -- --nocapture'
}
End-Log
Stop-Job $poller; Receive-Job $poller -ErrorAction SilentlyContinue | Out-File -Append -Encoding utf8 $procLog
Remove-Job $poller
Write-Host 'Done. Logs: logs\ext-diag2-summary.log, logs\ext-diag2.log, logs\ext-diag2-procs.log' -ForegroundColor Green
