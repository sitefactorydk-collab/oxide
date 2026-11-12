//! The managed Python environment: Settings → Advanced.

use oxide_core::PythonStatus;
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use crate::error::{CmdResult, CommandError};
use crate::state::AppState;

/// Where the environment stands (also how much space it takes).
#[tauri::command]
#[specta::specta]
pub async fn python_status(state: State<'_, AppState>) -> CmdResult<PythonStatus> {
    Ok(state.python.status(true).await)
}

/// Installs the environment now, as the first `run_python` would. Returns at once; progress
/// arrives as `pythonChanged` events.
#[tauri::command]
#[specta::specta]
pub async fn python_setup(state: State<'_, AppState>) -> CmdResult<()> {
    let python = state.python.clone();
    tauri::async_runtime::spawn(async move {
        // A failure is in the status (and the event) the page shows.
        let _ = python.ensure().await;
    });
    Ok(())
}

/// Throws the virtual environment away and installs it again (downloaded Pythons and the
/// package cache stay). Returns at once; progress arrives as `pythonChanged` events.
#[tauri::command]
#[specta::specta]
pub async fn python_repair(state: State<'_, AppState>) -> CmdResult<()> {
    if state.python.running_scripts() > 0 {
        return Err(CommandError::Invalid(
            "A Python script is still running. Try again when it has finished.".into(),
        ));
    }
    let python = state.python.clone();
    // Chats' browsers run from the environment: they close first.
    state.browser.shutdown().await;
    tauri::async_runtime::spawn(async move {
        if let Err(e) = python.repair().await {
            tracing::warn!("Python repair: {e}");
        }
    });
    Ok(())
}

/// Removes the environment, the downloaded Pythons and the cache. Chats' working folders
/// stay.
#[tauri::command]
#[specta::specta]
pub async fn python_reset(state: State<'_, AppState>) -> CmdResult<()> {
    // Chats' browsers run from the environment: they close first.
    state.browser.shutdown().await;
    state
        .python
        .reset()
        .await
        .map_err(|e| CommandError::Invalid(e.to_string()))
}

/// Opens the environment's folder in the file manager.
#[tauri::command]
#[specta::specta]
pub async fn open_python_folder(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    let root = state.python.root().to_path_buf();
    tokio::fs::create_dir_all(&root).await.map_err(|e| {
        tracing::warn!("couldn't create the Python folder: {e}");
        CommandError::Internal("The folder couldn't be created.".into())
    })?;
    app.opener()
        .open_path(root.to_string_lossy(), None::<&str>)
        .map_err(|e| {
            tracing::warn!("couldn't open the Python folder: {e}");
            CommandError::Internal("The folder couldn't be opened.".into())
        })
}
