# Commits the Phase 26 close-out notes (PLAN.md, PROGRESS.md) by name with logs\commit-msg.txt.
. "$PSScriptRoot\_common.ps1"
Start-Log 'commit'
Run 'git add' 'git add -- PLAN.md PROGRESS.md scripts/win/docs26.cmd scripts/win/docs26.ps1'
Run 'git commit' "git commit -F `"$(Join-Path $Root 'logs\commit-msg.txt')`""
Run 'git log' 'git log --oneline -n 3'
End-Log
