# Releasing Oxide

Installers, in-app updates and CI (Phase 17).

## What gets built

| Platform | Files | Notes |
|---|---|---|
| Windows x64 | `Oxide_<v>_x64-setup.exe` (NSIS, per user, no admin), `Oxide_<v>_x64_en-US.msi` (per machine) | Both download the WebView2 runtime when it's missing (Windows 11 has it). |
| macOS | `Oxide_<v>_universal.dmg` (Apple Silicon and Intel, macOS 11+) | Signed ad hoc unless the Apple secrets below are set. |
| Linux x64 | `Oxide_<v>_amd64.AppImage`, `Oxide_<v>_amd64.deb`, `Oxide-<v>-1.x86_64.rpm` | Built on Ubuntu 24.04 (PipeWire 1.x is needed), so they run on 24.04 and newer. |
| All | `latest.json` and a `.sig` per update bundle (the Windows installers themselves, `Oxide.app.tar.gz`, the AppImage) | For the in-app updater. |
| Chrome | `oxide-chrome-extension-<v>.zip` | The installers also carry the unpacked extension; Settings → Chrome shows its folder. |

Every installer bundles the `uv` sidecar for its platform (`scripts/uv-pins.json`) and the built
Chrome extension (`bundle.resources`). Python and its packages are installed on first use, as
before.

## One-time setup

1. **A GitHub repository.** Create one and push `main`. It must be **public** for in-app
   updates: GitHub serves a private repo's release files (`latest.json`, the installers) only
   to signed-in users, so installed copies can't fetch them. (To keep the code private, publish
   releases to a separate public repo instead.)
   ```
   git remote add origin https://github.com/<you>/oxide.git
   git push -u origin main
   ```
   The **CI** workflow (`.github/workflows/ci.yml`) then runs fmt, prettier, clippy, cargo test,
   typecheck, ESLint and Vitest on Windows, macOS and Ubuntu for every push and pull request.
2. **The update signing key.** `scripts\win\phase17.cmd` made it (`node scripts/updater-key.mjs`):
   the private key is `%USERPROFILE%\.tauri\oxide-updater.key` (no password), the public key is in
   `src-tauri/tauri.conf.json` → `plugins.updater.pubkey`. **Back the private key up**: without it,
   installed copies can't verify any later update.
3. **Repository secrets** (GitHub → Settings → Secrets and variables → Actions):
   - `TAURI_SIGNING_PRIVATE_KEY`: the contents of `oxide-updater.key` (PowerShell:
     `Get-Content $env:USERPROFILE\.tauri\oxide-updater.key -Raw`).
   - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: leave empty (create it with an empty value or skip it).
   - Optional, macOS signing and notarization: `APPLE_CERTIFICATE` (base64 of the .p12),
     `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY` (`Developer ID Application: …`),
     `APPLE_ID`, `APPLE_PASSWORD` (an app-specific password), `APPLE_TEAM_ID`. Needs a paid Apple
     Developer account.

## Making a release

1. Raise the version in `src-tauri/tauri.conf.json`, `package.json` and the workspace
   `Cargo.toml` (and `extension/static/manifest.json` if the extension changed). Commit.
2. Tag and push: `git tag v0.2.0 && git push origin v0.2.0` (or run the **Release** workflow by
   hand from the Actions tab).
3. The workflow builds on all three systems and fills a **draft** release named `Oxide v0.2.0`
   with the installers, `latest.json` and the extension zip.
4. Test the installers (below), then **Publish** the draft. Installed copies find the update at
   `https://github.com/<you>/<repo>/releases/latest/download/latest.json` (compiled in as
   `OXIDE_UPDATE_URL`) within six hours, or at once with Settings → About → **Check for updates**.

Drafts and pre-releases are invisible to the updater; only the latest published release counts.

## Testing on clean machines (the Phase 17 exit check)

- **Windows 11** (a fresh VM or Windows Sandbox): run the `-setup.exe`. Without code signing
  SmartScreen says “Windows protected your PC” → **More info** → **Run anyway**. Start Oxide from
  the Start menu, add a provider key, send a message, open Settings → Chrome (the extension folder
  is under the install folder), Settings → Data → Export all. Uninstall from Settings → Apps.
  Try the `.msi` too.
- **macOS** (a fresh VM, e.g. UTM or Tart): open the `.dmg`, drag Oxide to Applications. An
  ad-hoc signed app downloaded from the internet is quarantined: right-click → **Open** → **Open**,
  or `xattr -dr com.apple.quarantine /Applications/Oxide.app`. With the Apple secrets set it is
  signed and notarized and opens normally.
- **Ubuntu 24.04 or newer**: `sudo apt install ./Oxide_<v>_amd64.deb` (pulls in WebKitGTK), or
  `chmod +x Oxide_<v>_amd64.AppImage && ./Oxide_<v>_amd64.AppImage` (AppImages need `libfuse2t64`). Fedora: `sudo dnf install ./Oxide-<v>-1.x86_64.rpm`.
- **Updates**: install release *N*, publish *N+1*, then Settings → About → Check for updates →
  **Install … and restart**. Windows' installer closes Oxide, updates it and starts it again;
  macOS and the AppImage replace themselves and restart. `.deb`/`.rpm` installs say to update
  with the package manager.

## Code signing (optional)

- **Windows**: unsigned installers work but show SmartScreen. To sign, add a certificate (or
  Azure Trusted Signing) and set `bundle.windows.certificateThumbprint` / `signCommand` in
  `tauri.windows.conf.json`; see Tauri's “Windows code signing” guide.
- **macOS**: see the secrets above. Without them the app is signed ad hoc
  (`bundle.macOS.signingIdentity: "-"`), which Apple Silicon needs to run it at all.
- **Update signatures** are separate from code signing and always on: the updater refuses a
  bundle whose signature doesn't match `plugins.updater.pubkey`.

## Building installers locally (Windows)

`scripts\win\phase17.cmd` does it: `npx tauri build` with the signing key in
`TAURI_SIGNING_PRIVATE_KEY` → `target\release\bundle\nsis\` and `msi\`. A local build has no
update URL, so Settings → About says it doesn't look for updates.
