# Phase 2 helper: installs npm deps and bundles the Rust sources the agent needs to build
# oxide-core / oxide-store / oxide-llm on its Linux side (which has no crates.io access).
# Then runs every check once (no commit) so the agent can read Windows results.
# Output: logs\linux-vendor.tgz (git-ignored). Logs: logs\phase2-fetch*.log, logs\check*.log
. "$PSScriptRoot\_common.ps1"
Start-Log 'phase2-fetch'

Run 'npm install' 'npm install --no-fund --no-audit'

$tmp = Join-Path $env:TEMP 'oxide-linux-vendor'
if (Test-Path $tmp) { Remove-Item -Recurse -Force $tmp }
New-Item -ItemType Directory -Force -Path (Join-Path $tmp 'src') | Out-Null
Set-Content -Encoding ascii -Path (Join-Path $tmp 'src\lib.rs') -Value '// placeholder'
$manifest = @'
[package]
name = "oxide-linux-deps"
version = "0.0.0"
edition = "2024"
rust-version = "1.90"
publish = false

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "fmt"] }
tokio = { version = "1", features = ["full"] }
uuid = { version = "1", features = ["v7", "serde"] }
specta = "=2.0.0-rc.22"
specta-typescript = "0.0.9"
sqlx = { version = "0.8", default-features = false, features = ["runtime-tokio", "sqlite", "migrate", "macros"] }
reqwest = { version = "0.13", default-features = false, features = ["json", "native-tls"] }
tempfile = "3"

[target.'cfg(windows)'.dependencies]
keyring = { version = "3", features = ["windows-native"] }
[target.'cfg(target_os = "macos")'.dependencies]
keyring = { version = "3", features = ["apple-native"] }
[target.'cfg(target_os = "linux")'.dependencies]
keyring = { version = "3", features = ["linux-native"] }
'@
Set-Content -Encoding ascii -Path (Join-Path $tmp 'Cargo.toml') -Value $manifest
Copy-Item (Join-Path $Root 'Cargo.lock') (Join-Path $tmp 'Cargo.lock')

Push-Location $tmp
Run 'cargo vendor' 'cargo vendor --versioned-dirs vendor > vendor-config.toml'
Run 'cargo tree' 'cargo tree --target all -e normal --depth 1'
Pop-Location

# Read-only copies of crates the agent wants to read (already in the cargo cache).
$extra = Join-Path $tmp 'extra'
New-Item -ItemType Directory -Force -Path $extra | Out-Null
$reg = Get-ChildItem (Join-Path $env:USERPROFILE '.cargo\registry\src') -Directory | Select-Object -First 1
foreach ($c in 'tauri-specta-2.0.0-rc.21', 'specta-typescript-0.0.9', 'specta-serde-0.0.9') {
  $src = Join-Path $reg.FullName $c
  if (Test-Path $src) { Copy-Item -Recurse $src (Join-Path $extra $c) }
  Add-Content -Encoding utf8 -Path $script:LogFile -Value "extra $c : $(Test-Path $src)"
}
Copy-Item (Join-Path $tmp 'Cargo.lock') (Join-Path $tmp 'deps-Cargo.lock')

$out = Join-Path $Root 'logs\linux-vendor.tgz'
if (Test-Path $out) { Remove-Item -Force $out }
Run 'tar' "tar -czf `"$out`" -C `"$tmp`" vendor extra vendor-config.toml deps-Cargo.lock Cargo.toml"
Add-Content -Encoding utf8 -Path $script:SummaryFile -Value ("size MB: {0:N1}" -f ((Get-Item $out).Length / 1MB))
End-Log

# Stop a running dev app (it locks oxide.exe), then run all checks. No commit.
Get-Process -Name oxide -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
& "$PSScriptRoot\check.ps1"
