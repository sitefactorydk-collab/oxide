//! Projects.

use oxide_core::{NewProject, Project, ProjectPatch};
use tauri::State;

use crate::error::CmdResult;
use crate::state::AppState;

#[tauri::command]
#[specta::specta]
pub async fn list_projects(state: State<'_, AppState>) -> CmdResult<Vec<Project>> {
    Ok(state.store.list_projects().await?)
}

#[tauri::command]
#[specta::specta]
pub async fn get_project(state: State<'_, AppState>, id: String) -> CmdResult<Project> {
    Ok(state.store.get_project(&id).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn create_project(state: State<'_, AppState>, input: NewProject) -> CmdResult<Project> {
    Ok(state.store.create_project(input).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn update_project(
    state: State<'_, AppState>,
    id: String,
    patch: ProjectPatch,
) -> CmdResult<Project> {
    Ok(state.store.update_project(&id, patch).await?)
}

/// Deletes a project; its chats stay, outside any project.
#[tauri::command]
#[specta::specta]
pub async fn delete_project(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    Ok(state.store.delete_project(&id).await?)
}
