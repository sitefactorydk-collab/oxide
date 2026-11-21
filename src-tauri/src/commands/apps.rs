//! Utility Apps (Phase 20): the gallery, per-app settings, export and import, and the SDK
//! calls of the app frame (relayed by the main window, which shows the approval cards).

use std::sync::Arc;

use base64::Engine as _;
use oxide_core::{
    APP_FILE_EXT, AppBundle, AppCallError, AppCapability, AppImportPreview, AppPrefs, AppPrefsView,
    GenerationKind, UtilityApp,
};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;
use tauri_specta::Event;

use crate::error::{CmdResult, CommandError};
use crate::events::AppCallEvent;
use crate::state::AppState;

/// Every app: built-in first, then the user's own.
#[tauri::command]
#[specta::specta]
pub async fn list_apps(state: State<'_, AppState>) -> CmdResult<Vec<UtilityApp>> {
    Ok(state.apps.apps().list().await)
}

/// An app's manifest and source files, for the frame.
#[tauri::command]
#[specta::specta]
pub async fn app_bundle(state: State<'_, AppState>, id: String) -> CmdResult<AppBundle> {
    Ok(state.apps.apps().bundle(&id).await?)
}

/// Copies an app into “My apps”.
#[tauri::command]
#[specta::specta]
pub async fn clone_app(state: State<'_, AppState>, id: String) -> CmdResult<UtilityApp> {
    Ok(state.apps.apps().clone_app(&id).await?)
}

/// Removes an own app with its settings, storage and keys.
#[tauri::command]
#[specta::specta]
pub async fn delete_app(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    state.apps.apps().delete(&id).await?;
    // The app's sandbox container and browser go with it.
    let session = format!("app-{id}");
    state.sandboxes.forget(&session).await;
    state.browser.forget(&session).await;
    Ok(())
}

/// Saves one file of an own app (its Code view).
#[tauri::command]
#[specta::specta]
pub async fn save_app_file(
    state: State<'_, AppState>,
    id: String,
    path: String,
    content: String,
) -> CmdResult<UtilityApp> {
    Ok(state.apps.apps().save_file(&id, &path, &content).await?)
}

/// Strips characters file systems refuse.
fn file_name(name: &str, fallback: &str) -> String {
    let name: String = name
        .chars()
        .filter(|c| {
            !c.is_control() && !matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
        })
        .take(120)
        .collect();
    if name.trim().is_empty() {
        fallback.to_owned()
    } else {
        name.trim().to_owned()
    }
}

/// Asks where to save, then writes. `None`: the user cancelled.
async fn save_with_dialog(
    app: &AppHandle,
    title: &str,
    name: &str,
    bytes: Vec<u8>,
) -> CmdResult<Option<String>> {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_owned());
    let (tx, rx) = tokio::sync::oneshot::channel();
    let mut dialog = app.dialog().file().set_title(title).set_file_name(name);
    if let Some(ext) = &ext {
        dialog = dialog.add_filter(ext.to_uppercase(), &[ext.as_str()]);
    }
    dialog.save_file(move |p| {
        let _ = tx.send(p);
    });
    let picked = rx
        .await
        .map_err(|_| CommandError::Internal("the save dialog closed unexpectedly".into()))?;
    let Some(target) = picked.and_then(|p| p.into_path().ok()) else {
        return Ok(None);
    };
    tokio::fs::write(&target, bytes).await.map_err(|e| {
        tracing::warn!("couldn't save an app file: {e}");
        CommandError::Invalid(format!("The file couldn't be saved: {e}"))
    })?;
    Ok(Some(target.to_string_lossy().into_owned()))
}

/// Exports an app as one `.oxideapp` file where the user picks. Returns the path, or `None`
/// if they cancelled.
#[tauri::command]
#[specta::specta]
pub async fn export_app(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<Option<String>> {
    let bytes = state.apps.apps().export_bytes(&id).await?;
    let name = format!("{}.{APP_FILE_EXT}", file_name(&id, "app"));
    save_with_dialog(&app, "Export app", &name, bytes).await
}

/// Lets the user pick an `.oxideapp` file and reads it (nothing is installed yet). `None`:
/// they cancelled.
#[tauri::command]
#[specta::specta]
pub async fn pick_app_import(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<Option<AppImportPreview>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .set_title("Import an app")
        .add_filter("Oxide app", &[APP_FILE_EXT, "zip"])
        .pick_file(move |p| {
            let _ = tx.send(p);
        });
    let picked = rx
        .await
        .map_err(|_| CommandError::Internal("the file dialog closed unexpectedly".into()))?;
    let Some(path) = picked.and_then(|p| p.into_path().ok()) else {
        return Ok(None);
    };
    let meta = tokio::fs::metadata(&path)
        .await
        .map_err(|e| CommandError::Invalid(format!("The file can't be read: {e}")))?;
    if meta.len() > oxide_core::MAX_APP_BYTES + 1024 * 1024 {
        return Err(CommandError::Invalid(
            "The file is larger than 20 MB.".into(),
        ));
    }
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|e| CommandError::Invalid(format!("The file can't be read: {e}")))?;
    Ok(Some(state.apps.apps().preview_import(bytes).await?))
}

/// Installs an app read by [`pick_app_import`].
#[tauri::command]
#[specta::specta]
pub async fn install_app_import(
    state: State<'_, AppState>,
    token: String,
) -> CmdResult<UtilityApp> {
    Ok(state.apps.apps().install_import(&token).await?)
}

/// An app's settings dialog.
#[tauri::command]
#[specta::specta]
pub async fn app_prefs(state: State<'_, AppState>, id: String) -> CmdResult<AppPrefsView> {
    let apps = state.apps.apps();
    let manifest = apps.manifest(&id).await?;
    let prefs = apps.prefs(&id).await?;
    let text_model = state.apps.text_model(&manifest, &prefs).await;
    let keys = apps.keys(&id).await;
    let bytes = state
        .store
        .app_storage_bytes(&id)
        .await
        .map_err(|e| CommandError::Internal(e.to_string()))?;
    Ok(AppPrefsView {
        prefs,
        text_model,
        keys,
        storage_bytes: u32::try_from(bytes).unwrap_or(u32::MAX),
    })
}

/// Saves an app's settings. Grants can only be removed here.
#[tauri::command]
#[specta::specta]
pub async fn save_app_prefs(
    state: State<'_, AppState>,
    id: String,
    prefs: AppPrefs,
) -> CmdResult<AppPrefs> {
    Ok(state.apps.apps().save_prefs(&id, prefs).await?)
}

/// The user allowed a capability for an app (its approval card).
#[tauri::command]
#[specta::specta]
pub async fn grant_app_capability(
    state: State<'_, AppState>,
    id: String,
    capability: AppCapability,
) -> CmdResult<AppPrefs> {
    Ok(state.apps.apps().grant(&id, capability).await?)
}

/// Sets or (with `None`) removes an app's own key for a kind of generation.
#[tauri::command]
#[specta::specta]
pub async fn set_app_key(
    state: State<'_, AppState>,
    id: String,
    kind: GenerationKind,
    key: Option<String>,
) -> CmdResult<()> {
    Ok(state.apps.apps().set_key(&id, kind, key).await?)
}

/// Empties an app's storage.
#[tauri::command]
#[specta::specta]
pub async fn clear_app_storage(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    Ok(state.apps.apps().clear_storage(&id).await?)
}

/// Runs one SDK call of an app. `params_json` and the answer are JSON. Streamed text and
/// progress arrive as `appCallEvent`s with the same `call_id`.
#[tauri::command]
#[specta::specta]
pub async fn app_call(
    app: AppHandle,
    state: State<'_, AppState>,
    app_id: String,
    call_id: String,
    method: String,
    params_json: String,
) -> Result<String, AppCallError> {
    let params: serde_json::Value = serde_json::from_str(&params_json)
        .map_err(|e| AppCallError::failed(format!("the call's parameters aren't JSON: {e}")))?;
    let id = call_id.clone();
    let events = Arc::new(move |event: serde_json::Value| {
        let _ = AppCallEvent {
            call_id: id.clone(),
            event_json: event.to_string(),
        }
        .emit(&app);
    });
    let runtime = state.apps.clone();
    let value = runtime
        .call(&app_id, &call_id, &method, params, events)
        .await?;
    Ok(value.to_string())
}

/// Stops a running SDK call.
#[tauri::command]
#[specta::specta]
pub async fn cancel_app_call(state: State<'_, AppState>, call_id: String) -> CmdResult<()> {
    state.apps.cancel(&call_id);
    Ok(())
}

/// Saves a file an app made (`oxide.files.save`) where the user picks. `data_base64` is the
/// content. Returns the path, or `None` if they cancelled.
#[tauri::command]
#[specta::specta]
pub async fn save_app_output(
    app: AppHandle,
    name: String,
    data_base64: String,
) -> CmdResult<Option<String>> {
    if data_base64.len() > 400 * 1024 * 1024 {
        return Err(CommandError::Invalid(
            "The file is larger than 300 MB.".into(),
        ));
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data_base64.as_bytes())
        .map_err(|_| CommandError::Invalid("The file's data isn't valid.".into()))?;
    let name = file_name(&name, "download");
    save_with_dialog(&app, "Save", &name, bytes).await
}

/// A file dropped on the window, read for a Utility App's drop zone.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DroppedFile {
    pub name: String,
    pub media_type: String,
    pub data_base64: String,
}

/// Most files one drop hands to an app, and how large they may be.
const MAX_DROPPED_FILES: usize = 20;
const MAX_DROPPED_FILE: u64 = 50 * 1024 * 1024;
const MAX_DROPPED_TOTAL: u64 = 100 * 1024 * 1024;

/// The media type an app sees for a dropped file (by its extension).
fn dropped_media_type(path: &std::path::Path) -> &'static str {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "txt" => "text/plain",
        "md" | "markdown" => "text/markdown",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "csv" => "text/csv",
        "xml" => "application/xml",
        "json" => "application/json",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "mp4" => "video/mp4",
        _ => "application/octet-stream",
    }
}

/// Reads files the user dropped on the main window, for the app frame the drop landed on
/// (Tauri gives the window paths, never the page's own drop event). Only paths Oxide saw
/// dropped or picked are read; folders are skipped.
#[tauri::command]
#[specta::specta]
pub async fn read_dropped_files(app: AppHandle, paths: Vec<String>) -> CmdResult<Vec<DroppedFile>> {
    let chosen = |p: &std::path::Path| {
        app.try_state::<crate::chosen::ChosenPaths>()
            .is_some_and(|c| c.contains(p))
    };
    let mut out = Vec::new();
    let mut total = 0u64;
    for raw in paths.into_iter().take(MAX_DROPPED_FILES) {
        let path = std::path::PathBuf::from(&raw);
        if !chosen(&path) {
            // The window event that remembers drops can arrive just after the page's.
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            if !chosen(&path) {
                tracing::warn!("an app frame asked for a file that wasn't dropped");
                return Err(CommandError::Invalid(
                    "Only files dropped on Oxide can be handed to an app.".into(),
                ));
            }
        }
        let meta = tokio::fs::metadata(&path)
            .await
            .map_err(|_| CommandError::NotFound("The dropped file".into()))?;
        if meta.is_dir() {
            continue;
        }
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "file".into());
        if meta.len() > MAX_DROPPED_FILE {
            return Err(CommandError::Invalid(format!(
                "{name} is larger than 50 MB."
            )));
        }
        total += meta.len();
        if total > MAX_DROPPED_TOTAL {
            return Err(CommandError::Invalid(
                "The dropped files are larger than 100 MB together.".into(),
            ));
        }
        let bytes = tokio::fs::read(&path).await.map_err(|e| {
            tracing::warn!("a dropped file couldn't be read: {e}");
            CommandError::Invalid(format!("{name} couldn't be read."))
        })?;
        out.push(DroppedFile {
            media_type: dropped_media_type(&path).to_owned(),
            name,
            data_base64: base64::engine::general_purpose::STANDARD.encode(bytes),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod drop_tests {
    use super::dropped_media_type;
    use std::path::Path;

    #[test]
    fn dropped_files_get_a_media_type_from_their_extension() {
        assert_eq!(dropped_media_type(Path::new("a/Photo.JPG")), "image/jpeg");
        assert_eq!(
            dropped_media_type(Path::new("plugin.zip")),
            "application/zip"
        );
        assert_eq!(dropped_media_type(Path::new("doc.pdf")), "application/pdf");
        assert_eq!(
            dropped_media_type(Path::new("README")),
            "application/octet-stream"
        );
    }
}
