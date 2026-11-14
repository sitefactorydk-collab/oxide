# Smoke-tests the Linux packages of a release in Docker Desktop (Linux containers):
#   .deb in ubuntu:24.04, .rpm in fedora:41, the AppImage in ubuntu:24.04.
# Each container installs the package, starts Oxide on a virtual screen (Xvfb) for 25 s,
# saves a screenshot and checks the log for "Oxide <version> started".
# Packages come from release-test\ (download them from the GitHub release first, or let
# this script do it with the GitHub CLI `gh` when it's installed and signed in).
# Results: logs\linux-test-summary.log, logs\linux-test-<target>.log and .png.
. "$PSScriptRoot\_common.ps1"
$dir = Join-Path $Root 'release-test'
New-Item -ItemType Directory -Force -Path $dir | Out-Null

docker info *> $null
if ($LASTEXITCODE -ne 0) { Write-Host 'Docker Desktop is not running. Start it and try again.' -ForegroundColor Red; exit 1 }

if (-not (Get-ChildItem $dir -Filter *.deb -ErrorAction SilentlyContinue)) {
  if (Get-Command gh -ErrorAction SilentlyContinue) {
    $tag = 'v' + ((Get-Content (Join-Path $Root 'src-tauri\tauri.conf.json') -Raw | ConvertFrom-Json).version)
    Write-Host "Downloading the Linux packages of $tag with gh"
    gh release download $tag --dir $dir --pattern '*.deb' --pattern '*.rpm' --pattern '*.AppImage' --clobber
  }
}
$deb = Get-ChildItem $dir -Filter *.deb | Select-Object -First 1
$rpm = Get-ChildItem $dir -Filter *.rpm | Select-Object -First 1
$app = Get-ChildItem $dir -Filter *.AppImage | Select-Object -First 1
if (-not ($deb -or $rpm -or $app)) {
  Write-Host "No packages in $dir. Download the .deb, .rpm and .AppImage from the GitHub release into that folder." -ForegroundColor Red
  exit 1
}

# What runs inside each container once the package is installed. WebKitGTK runs as root
# without its sandbox and without GPU compositing in a container.
$run = @'
export WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS=1 WEBKIT_DISABLE_COMPOSITING_MODE=1 LIBGL_ALWAYS_SOFTWARE=1 RUST_LOG=info
mkdir -p /root/.config /root/.local/share
dbus-run-session -- xvfb-run -a -s "-screen 0 1280x800x24" sh -c '
  ( "$0" > /out/run.log 2>&1 & echo $! > /tmp/pid )
  sleep 25
  import -window root /out/screen.png
  kill $(cat /tmp/pid) 2>/dev/null; sleep 1
' "$OXIDE_BIN"
cat /out/run.log
grep -q "started" /out/run.log && echo "RESULT: STARTED" || echo "RESULT: DID NOT START"
'@ -replace "`r", ''

Start-Log 'linux-test'
$targets = @()
if ($deb) { $targets += @{ name = 'ubuntu-deb'; image = 'ubuntu:24.04'; file = $deb.Name; bin = '/usr/bin/oxide';
  setup = "apt-get update -qq && DEBIAN_FRONTEND=noninteractive apt-get install -y -qq /pkg/$($deb.Name) xvfb xauth imagemagick dbus >/dev/null" } }
if ($rpm) { $targets += @{ name = 'fedora-rpm'; image = 'fedora:41'; file = $rpm.Name; bin = '/usr/bin/oxide';
  setup = "dnf install -y -q /pkg/$($rpm.Name) xorg-x11-server-Xvfb xorg-x11-xauth ImageMagick dbus-daemon dbus-tools procps-ng >/dev/null" } }
if ($app) { $targets += @{ name = 'ubuntu-appimage'; image = 'ubuntu:24.04'; file = $app.Name; bin = '/tmp/oxide-appimage';
  setup = "apt-get update -qq && DEBIAN_FRONTEND=noninteractive apt-get install -y -qq xvfb xauth imagemagick dbus libfuse2t64 >/dev/null && printf '#!/bin/sh\nexec /pkg/$($app.Name) --appimage-extract-and-run `"`$@`"\n' > /tmp/oxide-appimage && chmod +x /tmp/oxide-appimage /pkg/$($app.Name) 2>/dev/null; true" } }

foreach ($t in $targets) {
  $out = Join-Path $Root "logs\linux-test-$($t.name)"
  New-Item -ItemType Directory -Force -Path $out | Out-Null
  $script = "set -e; $($t.setup); export OXIDE_BIN=$($t.bin); $run"
  $scriptFile = Join-Path $out 'test.sh'
  [IO.File]::WriteAllText($scriptFile, ($script -replace "`r", ''))
  Run $t.name "docker run --rm -v `"$dir`":/pkg -v `"$out`":/out $($t.image) bash /out/test.sh"
  $text = Get-Content (Join-Path $out 'run.log') -Raw -ErrorAction SilentlyContinue
  $ok = $text -match 'started'
  $line = '{0,-28} {1}' -f "$($t.name) app", $(if ($ok) { 'STARTED (see screen.png)' } else { 'DID NOT START (see run.log)' })
  Add-Content -Encoding utf8 -Path $script:SummaryFile -Value $line
  if (-not $ok) { $script:Failures++ }
  Write-Host "   $line"
}
End-Log
Write-Host "Screenshots: logs\linux-test-<target>\screen.png"
