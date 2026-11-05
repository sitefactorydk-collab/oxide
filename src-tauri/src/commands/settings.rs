//! App settings (typed JSON in SQLite).

use oxide_core::AppSettings;
use tauri::State;

use crate::error::CmdResult;
use crate::state::AppState;

#[tauri::command]
#[specta::specta]
pub async fn get_settings(state: State<'_, AppState>) -> CmdResult<AppSettings> {
    Ok(state.store.app_settings().await?)
}

/// Replaces the settings; returns them as stored (trimmed).
#[tauri::command]
#[specta::specta]
pub async fn save_settings(
    state: State<'_, AppState>,
    settings: AppSettings,
) -> CmdResult<AppSettings> {
    Ok(state.store.save_app_settings(settings).await?)
}
