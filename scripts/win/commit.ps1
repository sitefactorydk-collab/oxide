. "$PSScriptRoot\_common.ps1"
Start-Log 'commit'
$msgFile = Join-Path $Root 'logs\commit-msg.txt'
if (-not (Test-Path $msgFile)) { Write-Host 'logs\commit-msg.txt missing; nothing to commit.' -ForegroundColor Yellow; exit 1 }
if (-not (Test-Path (Join-Path $Root '.git'))) { Run 'git init' 'git init -b main' }
$name = (git config user.name); $email = (git config user.email)
if (-not $name -or -not $email) {
  Write-Host 'No git identity configured; using a repo-local placeholder (change with git config).' -ForegroundColor Yellow
  git config user.name 'Oxide Developer'
  git config user.email 'oxide-dev@localhost'
}
Run 'git add' 'git add -A'
Run 'git commit' "git commit -F `"$msgFile`""
Run 'git log' 'git log --oneline -n 5'
End-Log
exit $script:Failures
