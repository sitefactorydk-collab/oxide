//! Memory (Settings → Memory): view, add, edit and delete what Oxide remembers.

use oxide_core::{MemoryItem, NewMemory};
use tauri::State;

use crate::error::CmdResult;
use crate::state::AppState;

/// Every memory item, global and per project, oldest first.
#[tauri::command]
#[specta::specta]
pub async fn list_memory(state: State<'_, AppState>) -> CmdResult<Vec<MemoryItem>> {
    Ok(state.store.list_memory().await?)
}

#[tauri::command]
#[specta::specta]
pub async fn add_memory(state: State<'_, AppState>, input: NewMemory) -> CmdResult<MemoryItem> {
    Ok(state.store.add_memory(input, None).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn update_memory(
    state: State<'_, AppState>,
    id: String,
    text: String,
) -> CmdResult<MemoryItem> {
    Ok(state.store.update_memory(&id, &text).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn delete_memory(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    Ok(state.store.delete_memory(&id).await?)
}

/// Deletes every item (`projectId`: only that project's). Returns how many went.
#[tauri::command]
#[specta::specta]
pub async fn clear_memory(
    state: State<'_, AppState>,
    project_id: Option<String>,
) -> CmdResult<u32> {
    let n = state.store.clear_memory(project_id.as_deref()).await?;
    Ok(u32::try_from(n).unwrap_or(u32::MAX))
}
