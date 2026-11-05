. "$PSScriptRoot\_common.ps1"
& "$PSScriptRoot\check-toolchain.ps1"
Start-Log 'deps'
Run 'cargo update' 'cargo update'
Run 'npm install' 'npm install --no-fund --no-audit'
End-Log
& "$PSScriptRoot\check.ps1"
if ($LASTEXITCODE -eq 0) {
  & "$PSScriptRoot\commit.ps1"
} else {
  Write-Host 'Checks failed: not committing. See logs\check-summary.log' -ForegroundColor Red
}
