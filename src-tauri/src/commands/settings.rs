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
/// start at login, the panic shortcut) and the sandbox's take effect at once.
#[tauri::command]
#[specta::specta]
pub async fn save_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: AppSettings,
) -> CmdResult<AppSettings> {
    let saved = state.store.save_app_settings(settings).await?;
    crate::desktop::apply_settings(&app, &saved.desktop);
    crate::desktop::apply_computer(&app, saved.computer.enabled && state.computer.is_some());
    state
        .computer_bar
        .set_display(saved.computer.display.clone());
    state.sandboxes.configure(&saved.sandbox);
    state.browser.configure(&saved.browser);
    state.chrome.configure(&saved.chrome).await;
    Ok(saved)
}
