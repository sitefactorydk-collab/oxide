//! Artifacts (Phase 8): the gallery, one artifact with its versions, delete, and download.

use oxide_core::artifact::file_name;
use oxide_core::{Artifact, ArtifactDetail, ArtifactSummary};
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::error::{CmdResult, CommandError};
use crate::state::AppState;

/// Every artifact, most recently changed first, with its chat's title.
#[tauri::command]
#[specta::specta]
pub async fn list_artifacts(state: State<'_, AppState>) -> CmdResult<Vec<ArtifactSummary>> {
    Ok(state.store.list_artifacts().await?)
}

/// A chat's artifacts, oldest first.
#[tauri::command]
#[specta::specta]
pub async fn conversation_artifacts(
    state: State<'_, AppState>,
    conversation_id: String,
) -> CmdResult<Vec<Artifact>> {
    Ok(state.store.conversation_artifacts(&conversation_id).await?)
}

/// An artifact with every version.
#[tauri::command]
#[specta::specta]
pub async fn get_artifact(state: State<'_, AppState>, id: String) -> CmdResult<ArtifactDetail> {
    Ok(state.store.get_artifact(&id).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn delete_artifact(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    Ok(state.store.delete_artifact(&id).await?)
}

/// Saves a version (`None`: the newest) to a file the user picks. Returns the path, or
/// `None` if the user cancelled.
#[tauri::command]
#[specta::specta]
pub async fn save_artifact_file(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    version: Option<u32>,
) -> CmdResult<Option<String>> {
    let v = state.store.artifact_version(&id, version).await?;
    let name = file_name(&v.title, v.kind, v.language.as_deref());
    let ext = v.kind.extension(v.language.as_deref());
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .set_title("Download artifact")
        .set_file_name(name)
        .add_filter(ext.to_uppercase(), &[ext])
        .save_file(move |path| {
            let _ = tx.send(path);
        });
    let picked = rx
        .await
        .map_err(|_| CommandError::Internal("the save dialog closed unexpectedly".into()))?;
    let Some(path) = picked.and_then(|p| p.into_path().ok()) else {
        return Ok(None);
    };
    tokio::fs::write(&path, v.content).await.map_err(|e| {
        tracing::warn!("couldn't write the artifact: {e}");
        CommandError::Invalid(format!("The file couldn't be saved: {e}"))
    })?;
    Ok(Some(path.to_string_lossy().into_owned()))
}
