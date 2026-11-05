. "$PSScriptRoot\_common.ps1"
& {
  Write-Output "Oxide toolchain check  $(Get-Date -Format s)"
  Step 'OS' { (Get-CimInstance Win32_OperatingSystem).Caption; [Environment]::OSVersion.VersionString }
  Step 'rustup' { rustup --version; rustup show active-toolchain }
  Step 'rustc' { rustc -V }
  Step 'cargo' { cargo -V }
  Step 'tauri-cli' { cargo tauri --version }
  Step 'node' { node -v }
  Step 'npm' { npm -v }
  Step 'git' { git --version }
  Step 'uv' { uv --version }
  Step 'MSVC build tools' {
    $vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
    if (Test-Path $vswhere) {
      & $vswhere -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -format value -property displayName
      & $vswhere -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -format value -property installationVersion
    } else { 'vswhere not found' }
  }
  Step 'WebView2' {
    $keys = 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}',
            'HKCU:\Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}'
    foreach ($k in $keys) { if (Test-Path $k) { "$k -> $((Get-ItemProperty $k).pv)" } }
  }
  Write-Output "`nDONE"
} *>&1 | Out-File -Encoding utf8 (Join-Path $Root 'logs\toolchain.log')
