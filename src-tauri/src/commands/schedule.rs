//! Scheduled tasks (Phase 15): create, edit, pause, run now and history.

use oxide_core::{ScheduledTask, TaskInput, TaskRun, TaskSchedule, Timestamp};
use tauri::{AppHandle, State};

use crate::error::CmdResult;
use crate::state::AppState;

/// Folders are checked (and made full paths) like a chat's, when the task is saved. Those
/// the task already has (`kept`) need no new confirmation.
async fn resolve_folders(
    app: &AppHandle,
    mut input: TaskInput,
    kept: &[String],
) -> CmdResult<TaskInput> {
    crate::chosen::remember(
        app,
        &kept.iter().map(std::path::PathBuf::from).collect::<Vec<_>>(),
    );
    let mut folders = Vec::new();
    for f in std::mem::take(&mut input.folders) {
        folders.push(crate::commands::workspace::resolve_folder(app.clone(), f).await?);
    }
    input.folders = folders;
    Ok(input)
}

/// Every task, by name, each with its latest run.
#[tauri::command]
#[specta::specta]
pub async fn list_scheduled_tasks(state: State<'_, AppState>) -> CmdResult<Vec<ScheduledTask>> {
    Ok(state.scheduler.list().await?)
}

#[tauri::command]
#[specta::specta]
pub async fn get_scheduled_task(
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<ScheduledTask> {
    Ok(state.scheduler.get(&id).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn create_scheduled_task(
    app: AppHandle,
    state: State<'_, AppState>,
    input: TaskInput,
) -> CmdResult<ScheduledTask> {
    let input = resolve_folders(&app, input, &[]).await?;
    Ok(state.scheduler.create(input).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn update_scheduled_task(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    input: TaskInput,
) -> CmdResult<ScheduledTask> {
    let kept = state.scheduler.get(&id).await?.folders;
    let input = resolve_folders(&app, input, &kept).await?;
    Ok(state.scheduler.update(&id, input).await?)
}

/// Pauses (`false`) or resumes a task.
#[tauri::command]
#[specta::specta]
pub async fn set_scheduled_task_enabled(
    state: State<'_, AppState>,
    id: String,
    enabled: bool,
) -> CmdResult<ScheduledTask> {
    Ok(state.scheduler.set_enabled(&id, enabled).await?)
}

/// Deletes a task and its history; the runs' chats stay.
#[tauri::command]
#[specta::specta]
pub async fn delete_scheduled_task(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    Ok(state.scheduler.delete(&id).await?)
}

/// Runs a task now, in a new chat. Returns the run (skipped if one is still going).
#[tauri::command]
#[specta::specta]
pub async fn run_scheduled_task(state: State<'_, AppState>, id: String) -> CmdResult<TaskRun> {
    Ok(state.scheduler.run_now(&id).await?)
}

/// A task's runs, newest first (at most `limit`, default 50).
#[tauri::command]
#[specta::specta]
pub async fn scheduled_task_runs(
    state: State<'_, AppState>,
    task_id: String,
    limit: Option<u32>,
) -> CmdResult<Vec<TaskRun>> {
    let limit = usize::try_from(limit.unwrap_or(50)).unwrap_or(50);
    Ok(state.scheduler.runs(&task_id, limit).await?)
}

/// The next few times a schedule runs, in the computer's time zone. Fails with a sentence
/// for the user when the schedule isn't valid.
#[tauri::command]
#[specta::specta]
pub async fn preview_schedule(
    state: State<'_, AppState>,
    schedule: TaskSchedule,
) -> CmdResult<Vec<Timestamp>> {
    Ok(state.scheduler.preview(&schedule, 3)?)
}

/// The scheduled run that wrote in a chat, if the chat is one.
#[tauri::command]
#[specta::specta]
pub async fn task_run_for_chat(
    state: State<'_, AppState>,
    conversation_id: String,
) -> CmdResult<Option<TaskRun>> {
    Ok(state.store.task_run_for_chat(&conversation_id).await?)
}
