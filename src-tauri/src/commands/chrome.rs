//! Settings → Chrome: the extension's bridge, pairing and paired browsers.

use oxide_core::{ChromeStatus, PairingCode};
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use crate::error::{CmdResult, CommandError};
use crate::state::AppState;

/// Whether Oxide listens, which Chrome is connected, the paired ones and a waiting code.
#[tauri::command]
#[specta::specta]
pub async fn chrome_status(state: State<'_, AppState>) -> CmdResult<ChromeStatus> {
    Ok(state.chrome.status())
}

/// Shows a new six-word code for the extension's popup (valid for 5 minutes).
#[tauri::command]
#[specta::specta]
pub async fn chrome_start_pairing(state: State<'_, AppState>) -> CmdResult<PairingCode> {
    if state.chrome.port().is_none() {
        return Err(CommandError::Invalid(
            "Oxide isn't listening for Chrome. Switch on “Let models use Chrome” first (and pick \
             a free port if it says the port is taken)."
                .into(),
        ));
    }
    Ok(state.chrome.start_pairing())
}

#[tauri::command]
#[specta::specta]
pub async fn chrome_cancel_pairing(state: State<'_, AppState>) -> CmdResult<()> {
    state.chrome.cancel_pairing();
    Ok(())
}

/// Removes a paired browser: its token stops working and it disconnects.
#[tauri::command]
#[specta::specta]
pub async fn chrome_forget(state: State<'_, AppState>, browser_id: String) -> CmdResult<()> {
    state.chrome.forget(&browser_id).await;
    Ok(())
}

/// Opens the folder to load unpacked in `chrome://extensions`.
#[tauri::command]
#[specta::specta]
pub async fn chrome_open_extension_folder(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<()> {
    let Some(dir) = state.chrome.status().extension_dir else {
        return Err(CommandError::Invalid(
            "The extension hasn't been built yet: run “npm run extension” in Oxide's folder."
                .into(),
        ));
    };
    app.opener().open_path(dir, None::<&str>).map_err(|e| {
        tracing::warn!("couldn't open the extension folder: {e}");
        CommandError::Internal("The folder couldn't be opened.".into())
    })
}
