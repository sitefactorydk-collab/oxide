# Phase 17 close-out: commits with logs\commit-msg.txt and pushes to GitHub (CI runs again).
. "$PSScriptRoot\_common.ps1"
& "$PSScriptRoot\commit.ps1"
Start-Log 'phase17-close-push'
Run 'git push' 'git push origin main'
End-Log
