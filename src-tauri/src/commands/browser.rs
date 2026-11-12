//! The browser agent: Settings → Browser and a chat's browser.

use oxide_core::BrowserStatus;
use tauri::State;

use crate::error::{CmdResult, CommandError};
use crate::state::AppState;

/// What is installed, which browser a chat gets, and which chats have one open.
#[tauri::command]
#[specta::specta]
pub async fn browser_status(state: State<'_, AppState>) -> CmdResult<BrowserStatus> {
    let settings = state.store.app_settings().await?;
    state.browser.configure(&settings.browser);
    Ok(state.browser.status().await)
}

/// Installs what the browser needs now, as its first use would: the packages and, when
/// needed (or `chromium`), Chromium. Returns at once; progress arrives as `browserChanged`
/// events.
#[tauri::command]
#[specta::specta]
pub async fn browser_setup(state: State<'_, AppState>, chromium: bool) -> CmdResult<()> {
    let settings = state.store.app_settings().await?;
    let browser = state.browser.clone();
    browser.configure(&settings.browser);
    tauri::async_runtime::spawn(async move {
        // A failure is in the status (and the event) the page shows.
        if let Err(e) = browser.setup(chromium).await {
            tracing::warn!("browser setup: {e}");
        }
    });
    Ok(())
}

/// Closes a chat's browser (the next browser call opens a new one).
#[tauri::command]
#[specta::specta]
pub async fn browser_close(state: State<'_, AppState>, conversation_id: String) -> CmdResult<()> {
    state.browser.close(&conversation_id).await;
    Ok(())
}

/// Removes the Chromium Oxide downloaded.
#[tauri::command]
#[specta::specta]
pub async fn browser_remove_chromium(state: State<'_, AppState>) -> CmdResult<()> {
    if !state.browser.open_chats().is_empty() {
        return Err(CommandError::Invalid(
            "A chat's browser is open. Close it first (or wait until it closes by itself).".into(),
        ));
    }
    state
        .browser
        .remove_chromium()
        .await
        .map_err(|e| CommandError::Invalid(e.to_string()))
}
