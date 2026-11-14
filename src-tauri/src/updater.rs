//! In-app updates (`tauri-plugin-updater`, Phase 17).
//!
//! Release builds made by CI know where to look: the release workflow sets
//! `OXIDE_UPDATE_URL` at build time to the repository's `latest.json`
//! (`https://github.com/<owner>/<repo>/releases/latest/download/latest.json`). Local and
//! debug builds have no update URL, so Settings → About says updates aren't set up. Update
//! bundles are signed; the public key is `plugins.updater.pubkey` in `tauri.conf.json`, and
//! an update with another signature is refused by the plugin.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use oxide_core::{UpdateInfo, UpdatePhase, UpdateStatus};
use tauri::{AppHandle, Manager};
use tauri_plugin_updater::UpdaterExt;
use tauri_specta::Event;

use crate::events::UpdateChanged;

/// Where this build looks for `latest.json` (set by the release workflow).
const ENDPOINT: Option<&str> = option_env!("OXIDE_UPDATE_URL");

/// The first automatic check waits this long after start.
const FIRST_CHECK: Duration = Duration::from_secs(60);
/// Then one every so often while Oxide runs.
const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);

/// The updater's state, managed by the app.
pub struct Updates {
    status: Mutex<UpdateStatus>,
    pending: tokio::sync::Mutex<Option<tauri_plugin_updater::Update>>,
    busy: AtomicBool,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Linux packages installed with apt/dnf are updated by the package manager; only an
/// AppImage replaces itself.
fn installs_itself() -> bool {
    !cfg!(target_os = "linux") || std::env::var_os("APPIMAGE").is_some()
}

impl Updates {
    pub fn new(app: &AppHandle) -> Self {
        let configured = ENDPOINT.is_some_and(|u| !u.trim().is_empty()) && !cfg!(debug_assertions);
        let mut status = UpdateStatus::new(
            app.package_info().version.to_string(),
            configured,
            installs_itself(),
        );
        status.note = if !configured {
            Some(
                "This build doesn't look for updates. Installers built by the release workflow do."
                    .into(),
            )
        } else if !status.can_install {
            Some(
                "Installed from a .deb or .rpm package: update Oxide with your package manager, \
                 or download the new version."
                    .into(),
            )
        } else {
            None
        };
        Self {
            status: Mutex::new(status),
            pending: tokio::sync::Mutex::new(None),
            busy: AtomicBool::new(false),
        }
    }

    pub fn status(&self) -> UpdateStatus {
        lock(&self.status).clone()
    }

    fn update(&self, app: &AppHandle, f: impl FnOnce(&mut UpdateStatus)) {
        let status = {
            let mut s = lock(&self.status);
            f(&mut s);
            s.clone()
        };
        if let Err(e) = UpdateChanged(status).emit(app) {
            tracing::debug!("couldn't send the update status: {e}");
        }
    }
}

fn updater(app: &AppHandle) -> Result<tauri_plugin_updater::Updater, String> {
    let url = ENDPOINT.ok_or("This build doesn't look for updates.")?;
    let url: tauri::Url = url
        .parse()
        .map_err(|e| format!("The update address is invalid: {e}"))?;
    app.updater_builder()
        .endpoints(vec![url])
        .and_then(|b| b.build())
        .map_err(|e| format!("The updater couldn't start: {e}"))
}

fn plain(e: &tauri_plugin_updater::Error) -> String {
    let text = e.to_string();
    let lower = text.to_ascii_lowercase();
    if lower.contains("signature") {
        "The update's signature didn't match, so it wasn't installed.".into()
    } else if lower.contains("network") || lower.contains("request") || lower.contains("connect") {
        format!("Couldn't reach the update server ({text}).")
    } else {
        format!("The update failed: {text}")
    }
}

/// Looks for a newer version. A check while another one (or a download) runs returns the
/// current status.
pub async fn check(app: &AppHandle) -> UpdateStatus {
    let Some(updates) = app.try_state::<Updates>() else {
        return UpdateStatus::new(app.package_info().version.to_string(), false, false);
    };
    if !updates.status().configured || updates.busy.swap(true, Ordering::SeqCst) {
        return updates.status();
    }
    updates.update(app, |s| {
        s.phase = UpdatePhase::Checking;
        s.error = None;
    });
    let result = match updater(app) {
        Ok(u) => u.check().await.map_err(|e| plain(&e)),
        Err(e) => Err(e),
    };
    let now = oxide_store::now_ms();
    match result {
        Ok(Some(update)) => {
            let info = UpdateInfo {
                version: update.version.clone(),
                date: update
                    .raw_json
                    .get("pub_date")
                    .and_then(|d| d.as_str())
                    .map(str::to_owned),
                notes: update.body.clone().filter(|b| !b.trim().is_empty()),
            };
            tracing::info!("update available: {}", info.version);
            *updates.pending.lock().await = Some(update);
            updates.update(app, |s| {
                s.phase = UpdatePhase::Available;
                s.available = Some(info);
                s.checked_at = Some(now);
            });
        }
        Ok(None) => updates.update(app, |s| {
            s.phase = UpdatePhase::UpToDate;
            s.available = None;
            s.checked_at = Some(now);
        }),
        Err(e) => {
            tracing::warn!("update check failed: {e}");
            updates.update(app, |s| {
                s.phase = UpdatePhase::Failed;
                s.error = Some(e);
                s.checked_at = Some(now);
            });
        }
    }
    updates.busy.store(false, Ordering::SeqCst);
    updates.status()
}

/// Downloads and installs the update found by [`check`], then restarts Oxide. On Windows
/// the installer closes Oxide itself and starts the new version.
pub async fn install(app: &AppHandle) -> Result<(), String> {
    let updates = app
        .try_state::<Updates>()
        .ok_or("The updater isn't running.")?;
    if !updates.status().can_install {
        return Err(
            "This install is updated by the system's package manager, not by Oxide.".into(),
        );
    }
    if updates.busy.swap(true, Ordering::SeqCst) {
        return Err("Oxide is already checking for or downloading an update.".into());
    }
    let Some(update) = updates.pending.lock().await.take() else {
        updates.busy.store(false, Ordering::SeqCst);
        return Err("Check for updates first.".into());
    };
    updates.update(app, |s| {
        s.phase = UpdatePhase::Downloading;
        s.downloaded = 0;
        s.total = None;
        s.error = None;
    });
    let progress_app = app.clone();
    let mut last_sent = 0u64;
    let result = update
        .download_and_install(
            move |chunk, total| {
                let Some(u) = progress_app.try_state::<Updates>() else {
                    return;
                };
                let done = {
                    let mut s = lock(&u.status);
                    s.downloaded += chunk as u64;
                    s.total = total;
                    s.downloaded
                };
                // At most one event per 256 KB.
                if done - last_sent >= 256 * 1024 || total == Some(done) {
                    last_sent = done;
                    u.update(&progress_app, |_| {});
                }
            },
            || {},
        )
        .await;
    updates.busy.store(false, Ordering::SeqCst);
    match result {
        Ok(()) => {
            tracing::info!("update installed; restarting");
            updates.update(app, |s| s.phase = UpdatePhase::Installing);
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(Duration::from_millis(500)).await;
                app.restart();
            });
            Ok(())
        }
        Err(e) => {
            let msg = plain(&e);
            tracing::warn!("update install failed: {e}");
            *updates.pending.lock().await = Some(update);
            updates.update(app, |s| {
                s.phase = UpdatePhase::Failed;
                s.error = Some(msg.clone());
            });
            Err(msg)
        }
    }
}

/// Checks shortly after start and then every few hours, while Settings → About →
/// "Check for updates automatically" is on.
pub fn start_background(app: &AppHandle) {
    let Some(updates) = app.try_state::<Updates>() else {
        return;
    };
    if !updates.status().configured {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_CHECK).await;
        loop {
            let on = match app.try_state::<crate::state::AppState>() {
                Some(state) => state
                    .store
                    .app_settings()
                    .await
                    .map(|s| s.updates.check_automatically)
                    .unwrap_or(true),
                None => false,
            };
            let phase = app
                .try_state::<Updates>()
                .map(|u| u.status().phase)
                .unwrap_or(UpdatePhase::Idle);
            if on && !matches!(phase, UpdatePhase::Downloading | UpdatePhase::Installing) {
                check(&app).await;
            }
            tokio::time::sleep(CHECK_EVERY).await;
        }
    });
}
