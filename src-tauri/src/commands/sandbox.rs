//! The Docker sandbox (Phase 12b): Settings → Sandbox and the session panel's Sandbox tab.

use oxide_core::{ChatSandbox, DockerStatus, SandboxListing};
use oxide_sandbox::SandboxError;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::error::{CmdResult, CommandError};
use crate::state::AppState;

impl From<SandboxError> for CommandError {
    fn from(e: SandboxError) -> Self {
        // Sandbox errors are written for users and never contain secrets.
        CommandError::Invalid(e.to_string())
    }
}

/// Whether Docker runs and the images are downloaded. `refresh` checks again now.
#[tauri::command]
#[specta::specta]
pub async fn sandbox_docker_status(
    state: State<'_, AppState>,
    refresh: bool,
) -> CmdResult<DockerStatus> {
    Ok(state.sandboxes.docker_status(refresh).await)
}

/// Downloads the sandbox's images when missing. Returns when done (or failed); the status
/// shows the download meanwhile (`sandboxChanged`).
#[tauri::command]
#[specta::specta]
pub async fn sandbox_prepare(state: State<'_, AppState>) -> CmdResult<()> {
    Ok(state.sandboxes.prepare().await?)
}

/// A chat's sandbox: whether it exists and runs, its workspace, running commands.
#[tauri::command]
#[specta::specta]
pub async fn chat_sandbox(
    state: State<'_, AppState>,
    conversation_id: String,
) -> CmdResult<ChatSandbox> {
    Ok(state.sandboxes.chat(&conversation_id).await)
}

/// One folder of a chat's workspace. `None` when the sandbox isn't running and `start` is
/// false.
#[tauri::command]
#[specta::specta]
pub async fn sandbox_files(
    state: State<'_, AppState>,
    conversation_id: String,
    path: String,
    start: bool,
) -> CmdResult<Option<SandboxListing>> {
    Ok(state.sandboxes.list(&conversation_id, &path, start).await?)
}

/// Stops a chat's sandbox now (its files stay).
#[tauri::command]
#[specta::specta]
pub async fn sandbox_stop(state: State<'_, AppState>, conversation_id: String) -> CmdResult<()> {
    Ok(state.sandboxes.stop(&conversation_id).await?)
}

/// Removes a chat's sandbox and every file in it.
#[tauri::command]
#[specta::specta]
pub async fn sandbox_reset(state: State<'_, AppState>, conversation_id: String) -> CmdResult<()> {
    Ok(state.sandboxes.reset(&conversation_id).await?)
}

/// Asks for a file (or a folder) on this computer and copies it into the chat's sandbox at
/// `dest`. The user picking it is the approval. Returns its path in the sandbox, or `None`
/// when cancelled.
#[tauri::command]
#[specta::specta]
pub async fn sandbox_copy_in(
    app: AppHandle,
    state: State<'_, AppState>,
    conversation_id: String,
    dest: String,
    folder: bool,
) -> CmdResult<Option<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let dialog = app.dialog().file().set_title(if folder {
        "Copy a folder into the sandbox"
    } else {
        "Copy a file into the sandbox"
    });
    if folder {
        dialog.pick_folder(move |p| {
            let _ = tx.send(p);
        });
    } else {
        dialog.pick_file(move |p| {
            let _ = tx.send(p);
        });
    }
    let picked = rx
        .await
        .map_err(|_| CommandError::Internal("the picker closed unexpectedly".into()))?;
    let Some(path) = picked.and_then(|p| p.into_path().ok()) else {
        return Ok(None);
    };
    Ok(Some(
        state
            .sandboxes
            .copy_in(&conversation_id, &path, &dest)
            .await?,
    ))
}

/// Asks for a folder on this computer and copies `path` from the chat's sandbox into it
/// (keeping its name; nothing is overwritten). Returns where it went, or `None` when
/// cancelled.
#[tauri::command]
#[specta::specta]
pub async fn sandbox_save(
    app: AppHandle,
    state: State<'_, AppState>,
    conversation_id: String,
    path: String,
) -> CmdResult<Option<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .set_title("Save from the sandbox to a folder")
        .pick_folder(move |p| {
            let _ = tx.send(p);
        });
    let picked = rx
        .await
        .map_err(|_| CommandError::Internal("the picker closed unexpectedly".into()))?;
    let Some(dir) = picked.and_then(|p| p.into_path().ok()) else {
        return Ok(None);
    };
    let to = state
        .sandboxes
        .copy_out(&conversation_id, &path, &dir)
        .await?;
    Ok(Some(to.to_string_lossy().into_owned()))
}
