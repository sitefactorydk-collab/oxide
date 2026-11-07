//! Agentic work in a chat (Phase 11): granted folders and the task list.

use oxide_agent::tools::files::resolve_grant;
use oxide_core::{ChatWorkspace, TaskList};
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use crate::error::{CmdResult, CommandError};
use crate::state::AppState;

/// Checks a folder the user picked and returns its resolved path (also before the chat
/// exists, for the new-chat composer).
#[tauri::command]
#[specta::specta]
pub async fn resolve_folder(path: String) -> CmdResult<String> {
    tokio::task::spawn_blocking(move || resolve_grant(&path))
        .await
        .map_err(|e| CommandError::Internal(e.to_string()))?
        .map_err(CommandError::Invalid)
}

/// A chat's folders and whether it may delete for good.
#[tauri::command]
#[specta::specta]
pub async fn chat_workspace(
    state: State<'_, AppState>,
    conversation_id: String,
) -> CmdResult<ChatWorkspace> {
    Ok(state.store.chat_workspace(&conversation_id).await?)
}

/// Grants a folder to a chat. Its file tools reach only paths inside granted folders.
#[tauri::command]
#[specta::specta]
pub async fn add_chat_folder(
    state: State<'_, AppState>,
    conversation_id: String,
    path: String,
) -> CmdResult<ChatWorkspace> {
    let resolved = resolve_folder(path).await?;
    Ok(state
        .store
        .add_chat_folder(&conversation_id, &resolved)
        .await?)
}

/// Removes a folder from a chat.
#[tauri::command]
#[specta::specta]
pub async fn remove_chat_folder(
    state: State<'_, AppState>,
    conversation_id: String,
    path: String,
) -> CmdResult<ChatWorkspace> {
    Ok(state
        .store
        .remove_chat_folder(&conversation_id, &path)
        .await?)
}

/// Lets `delete_file` delete for good in a chat (off: it moves things into `_to_delete`).
#[tauri::command]
#[specta::specta]
pub async fn set_chat_allow_delete(
    state: State<'_, AppState>,
    conversation_id: String,
    allow: bool,
) -> CmdResult<ChatWorkspace> {
    Ok(state
        .store
        .set_chat_allow_delete(&conversation_id, allow)
        .await?)
}

/// The chat's task list (empty when the model never made one).
#[tauri::command]
#[specta::specta]
pub async fn chat_task_list(
    state: State<'_, AppState>,
    conversation_id: String,
) -> CmdResult<TaskList> {
    Ok(state.store.task_list(&conversation_id).await?)
}

/// Opens one of a chat's folders in the file manager.
#[tauri::command]
#[specta::specta]
pub async fn open_chat_folder(
    app: AppHandle,
    state: State<'_, AppState>,
    conversation_id: String,
    path: String,
) -> CmdResult<()> {
    let ws = state.store.chat_workspace(&conversation_id).await?;
    if !ws.folders.iter().any(|f| f.path == path) {
        return Err(CommandError::NotFound("folder not found".into()));
    }
    app.opener().open_path(path, None::<&str>).map_err(|e| {
        tracing::warn!("couldn't open a folder: {e}");
        CommandError::Internal("The folder couldn't be opened.".into())
    })
}
