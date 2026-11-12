//! App settings (typed JSON in SQLite).

use oxide_core::AppSettings;
use tauri::{AppHandle, State};

use crate::error::CmdResult;
use crate::state::AppState;

#[tauri::command]
#[specta::specta]
pub async fn get_settings(state: State<'_, AppState>) -> CmdResult<AppSettings> {
    Ok(state.store.app_settings().await?)
}

/// Replaces the settings; returns them as stored (trimmed). Desktop settings (shortcut,
/// start at login) and the sandbox's take effect at once.
#[tauri::command]
#[specta::specta]
pub async fn save_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: AppSettings,
) -> CmdResult<AppSettings> {
    let saved = state.store.save_app_settings(settings).await?;
    crate::desktop::apply_settings(&app, &saved.desktop);
    state.sandboxes.configure(&saved.sandbox);
    Ok(saved)
}
