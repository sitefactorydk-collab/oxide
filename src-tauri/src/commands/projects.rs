//! Projects and their knowledge files.

use oxide_agent::knowledge::read_knowledge_file;
use oxide_core::project::knowledge_mode;
use oxide_core::{
    AttachmentSource, KnowledgeFile, KnowledgeKind, KnowledgeText, NewProject, Project,
    ProjectKnowledge, ProjectPatch,
};
use oxide_store::NewKnowledgeFile;
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

/// Renames, edits the description or instructions, or archives / unarchives a project.
#[tauri::command]
#[specta::specta]
pub async fn update_project(
    state: State<'_, AppState>,
    id: String,
    patch: ProjectPatch,
) -> CmdResult<Project> {
    Ok(state.store.update_project(&id, patch).await?)
}

/// Deletes a project and its knowledge; its chats stay, outside any project.
#[tauri::command]
#[specta::specta]
pub async fn delete_project(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    Ok(state.store.delete_project(&id).await?)
}

/// A project's knowledge files (without their text), their size and how chats use them.
#[tauri::command]
#[specta::specta]
pub async fn project_knowledge(
    state: State<'_, AppState>,
    project_id: String,
) -> CmdResult<ProjectKnowledge> {
    let files = state.store.knowledge_files(&project_id).await?;
    let settings = state.store.app_settings().await?;
    let total_tokens = files.iter().map(|f| f.tokens).fold(0, u32::saturating_add);
    let budget_tokens = settings.projects.knowledge_budget;
    Ok(ProjectKnowledge {
        mode: knowledge_mode(total_tokens, files.len(), budget_tokens),
        files,
        total_tokens,
        budget_tokens,
    })
}

/// Adds a file (a path from the picker or drop, or pasted bytes) to a project's knowledge.
/// Text and code files and PDFs are accepted; their text is stored and indexed.
#[tauri::command]
#[specta::specta]
pub async fn add_project_file(
    state: State<'_, AppState>,
    project_id: String,
    source: AttachmentSource,
) -> CmdResult<KnowledgeFile> {
    let file = read_knowledge_file(&state.attachments, source).await?;
    Ok(state.store.add_knowledge_file(&project_id, file).await?)
}

/// Adds typed or pasted text to a project's knowledge ("Add text content").
#[tauri::command]
#[specta::specta]
pub async fn add_project_text(
    state: State<'_, AppState>,
    project_id: String,
    name: String,
    text: String,
) -> CmdResult<KnowledgeFile> {
    let file = NewKnowledgeFile {
        name,
        kind: KnowledgeKind::Note,
        media_type: "text/plain".into(),
        pages: None,
        note: None,
        text,
    };
    Ok(state.store.add_knowledge_file(&project_id, file).await?)
}

/// One knowledge file with its text (the viewer).
#[tauri::command]
#[specta::specta]
pub async fn get_knowledge_file(
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<KnowledgeText> {
    Ok(state.store.knowledge_file(&id).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn delete_knowledge_file(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    Ok(state.store.delete_knowledge_file(&id).await?)
}
