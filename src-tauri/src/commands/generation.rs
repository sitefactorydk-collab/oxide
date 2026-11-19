//! Settings → Media generation (Phase 19): keys of its own per kind, Test, the model list, and the
//! generated files the chat shows (players and downloads).

use base64::Engine;
use oxide_core::{GenerationEndpoint, GenerationKind, GenerationTest};
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::error::{CmdResult, CommandError};
use crate::state::AppState;

/// The kinds of generation that have a key of their own in the keychain.
#[tauri::command]
#[specta::specta]
pub async fn generation_keys(state: State<'_, AppState>) -> CmdResult<Vec<GenerationKind>> {
    let g = state.generation.clone();
    tokio::task::spawn_blocking(move || g.keys())
        .await
        .map_err(|e| CommandError::Internal(e.to_string()))
}

/// Saves a kind's own key (it overrides the endpoint's), or removes it with `None`.
#[tauri::command]
#[specta::specta]
pub async fn set_generation_key(
    state: State<'_, AppState>,
    kind: GenerationKind,
    key: Option<String>,
) -> CmdResult<()> {
    let g = state.generation.clone();
    tokio::task::spawn_blocking(move || g.set_key(kind, key.as_deref()))
        .await
        .map_err(|e| CommandError::Internal(e.to_string()))?
        .map_err(CommandError::Keychain)
}

/// Test in Settings → Media generation: reaches the endpoint and looks for the model in its list.
#[tauri::command]
#[specta::specta]
pub async fn test_generation(
    state: State<'_, AppState>,
    kind: GenerationKind,
    endpoint: GenerationEndpoint,
) -> CmdResult<GenerationTest> {
    Ok(state.generation.test(kind, &endpoint.normalized()).await)
}

/// Every model the endpoint lists, for the model picker.
#[tauri::command]
#[specta::specta]
pub async fn generation_models(
    state: State<'_, AppState>,
    kind: GenerationKind,
    endpoint: GenerationEndpoint,
) -> CmdResult<Vec<String>> {
    state
        .generation
        .models(kind, &endpoint.normalized())
        .await
        .map_err(CommandError::Invalid)
}

/// A stored file (generated video or audio) as base64, for a player in the window. Only files
/// in the attachments folder can be read.
#[tauri::command]
#[specta::specta]
pub async fn attachment_media(state: State<'_, AppState>, path: String) -> CmdResult<String> {
    let bytes = state.attachments.read_file(&path).await?;
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
}

/// Saves a stored file (a generated image's full-size original, video or audio) where the user
/// picks. Returns the path, or `None` if they cancelled.
#[tauri::command]
#[specta::specta]
pub async fn save_attachment_file(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    name: String,
) -> CmdResult<Option<String>> {
    let bytes = state.attachments.read_file(&path).await?;
    let name: String = name
        .chars()
        .filter(|c| {
            !c.is_control() && !matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
        })
        .take(120)
        .collect();
    let name = if name.trim().is_empty() {
        "generated".to_owned()
    } else {
        name.trim().to_owned()
    };
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_owned());
    let (tx, rx) = tokio::sync::oneshot::channel();
    let mut dialog = app
        .dialog()
        .file()
        .set_title("Download")
        .set_file_name(&name);
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
        tracing::warn!("couldn't save a generated file: {e}");
        CommandError::Invalid(format!("The file couldn't be saved: {e}"))
    })?;
    Ok(Some(target.to_string_lossy().into_owned()))
}
