//! Settings → About → Updates (Phase 17).

use oxide_core::UpdateStatus;
use tauri::{AppHandle, State};

use crate::error::{CmdResult, CommandError};
use crate::updater::Updates;

/// The updater's state.
#[tauri::command]
#[specta::specta]
pub fn update_status(updates: State<'_, Updates>) -> UpdateStatus {
    updates.status()
}

/// Looks for a newer version now.
#[tauri::command]
#[specta::specta]
pub async fn check_for_update(app: AppHandle) -> CmdResult<UpdateStatus> {
    Ok(crate::updater::check(&app).await)
}

/// Downloads and installs the version found by the last check, then restarts Oxide.
#[tauri::command]
#[specta::specta]
pub async fn install_update(app: AppHandle) -> CmdResult<()> {
    crate::updater::install(&app)
        .await
        .map_err(CommandError::Invalid)
}
