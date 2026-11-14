//! Settings → Data: where Oxide keeps its files, "Export all" and "Delete all" (Phase 17).

use std::sync::Mutex;

use oxide_agent::data::{DataError, DeleteAll, export_all, folder_size};
use oxide_core::{DELETE_ALL_CONFIRMATION, DataDeleted, DataExport, DataLocations};
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

use crate::error::{CmdResult, CommandError};
use crate::state::AppState;

/// The zip the last "Export all" wrote, so "Show in folder" only ever reveals that file.
#[derive(Default)]
pub struct LastExport(Mutex<Option<std::path::PathBuf>>);

impl From<DataError> for CommandError {
    fn from(e: DataError) -> Self {
        match e {
            DataError::Store(s) => s.into(),
            DataError::Invalid(m) => CommandError::Invalid(m),
            DataError::Io(m) => {
                tracing::warn!("data: {m}");
                let mut chars = m.chars();
                let text = match chars.next() {
                    Some(f) => f.to_uppercase().chain(chars).collect(),
                    None => m,
                };
                CommandError::Invalid(text)
            }
        }
    }
}

/// Oxide's data, config and log folders, and how much the data folder holds.
#[tauri::command]
#[specta::specta]
pub async fn data_locations(state: State<'_, AppState>) -> CmdResult<DataLocations> {
    let dirs = state.dirs.clone();
    let data_bytes = tokio::task::spawn_blocking({
        let data = dirs.data.clone();
        move || folder_size(&data)
    })
    .await
    .ok();
    Ok(DataLocations {
        data: dirs.data.to_string_lossy().into_owned(),
        config: dirs.config.to_string_lossy().into_owned(),
        logs: dirs.logs.to_string_lossy().into_owned(),
        data_bytes,
    })
}

/// Opens the data folder in the file manager.
#[tauri::command]
#[specta::specta]
pub async fn open_data_folder(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    app.opener()
        .open_path(state.dirs.data.to_string_lossy(), None::<&str>)
        .map_err(|e| {
            tracing::warn!("couldn't open the data folder: {e}");
            CommandError::Internal("The folder couldn't be opened.".into())
        })
}

/// Asks where to save, then writes everything into one zip. `None` when the dialog was
/// cancelled.
#[tauri::command]
#[specta::specta]
pub async fn export_all_data(
    app: AppHandle,
    state: State<'_, AppState>,
    last: State<'_, LastExport>,
) -> CmdResult<Option<DataExport>> {
    let day = chrono::Local::now().date_naive();
    let (tx, rx) = tokio::sync::oneshot::channel();
    let mut dialog = app
        .dialog()
        .file()
        .set_title("Export all data")
        .set_file_name(format!("oxide-export-{day}.zip"))
        .add_filter("Zip archive", &["zip"]);
    if let Ok(downloads) = tauri::Manager::path(&app).download_dir() {
        dialog = dialog.set_directory(downloads);
    }
    dialog.save_file(move |path| {
        let _ = tx.send(path);
    });
    let picked = rx
        .await
        .map_err(|_| CommandError::Internal("the save dialog closed unexpectedly".into()))?;
    let Some(mut path) = picked.and_then(|p| p.into_path().ok()) else {
        return Ok(None);
    };
    if path.extension().is_none() {
        path.set_extension("zip");
    }
    let done = export_all(&state.store, &state.dirs, &path).await?;
    tracing::info!(
        "exported {} tables and {} files ({} bytes)",
        done.tables,
        done.files,
        done.bytes
    );
    *last.0.lock().unwrap_or_else(|e| e.into_inner()) = Some(path);
    Ok(Some(done))
}

/// Shows the last export in the file manager.
#[tauri::command]
#[specta::specta]
pub async fn reveal_data_export(app: AppHandle, last: State<'_, LastExport>) -> CmdResult<()> {
    let path = last
        .0
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .ok_or_else(|| CommandError::NotFound("export not found".into()))?;
    app.opener().reveal_item_in_dir(&path).map_err(|e| {
        tracing::warn!("couldn't show the export: {e}");
        CommandError::Internal("The folder couldn't be opened.".into())
    })
}

/// Deletes every chat, project, memory, setting, file and keychain entry Oxide made, then
/// restarts Oxide. `confirm` must be the word the dialog asks for.
#[tauri::command]
#[specta::specta]
pub async fn delete_all_data(
    app: AppHandle,
    state: State<'_, AppState>,
    confirm: String,
) -> CmdResult<DataDeleted> {
    if !confirm.trim().eq_ignore_ascii_case(DELETE_ALL_CONFIRMATION) {
        return Err(CommandError::Invalid(format!(
            "Type “{DELETE_ALL_CONFIRMATION}” to confirm."
        )));
    }
    tracing::info!("deleting all data");
    let _ = crate::computer::stop_all(&app);
    // Start at login off and the shortcuts back to their defaults, before the settings go.
    crate::desktop::apply_settings(&app, &oxide_core::DesktopSettings::default());
    crate::desktop::apply_computer(&app, false);
    let done = DeleteAll {
        store: &state.store,
        secrets: &state.secrets,
        dirs: &state.dirs,
        chat: &state.chat,
        mcp: &state.mcp,
        extensions: &state.extensions,
        chrome: &state.chrome,
        sandboxes: &state.sandboxes,
        browser: &state.browser,
    }
    .run()
    .await?;
    tracing::info!(
        "deleted all data ({} keychain entries; {} left for the next start); restarting",
        done.secrets,
        done.leftovers.len()
    );
    // Give the window a moment to show the result and clear its own storage.
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
        handle.restart();
    });
    Ok(done)
}
