//! The Utility App builder (Phase 21): open an app's draft, change its files, checkpoints,
//! Save to Utility Apps, and the Preview's answers to the builder model's questions.

use oxide_core::{AppCheckpoint, AppDraft, PreviewReport, UtilityApp};
use serde_json::json;
use tauri::State;

use crate::error::CmdResult;
use crate::state::AppState;

/// Opens the builder on an own app (`id`) or a new app from the template (`None`). The
/// builder chat is made with `model` the first time.
#[tauri::command]
#[specta::specta]
pub async fn open_app_builder(
    state: State<'_, AppState>,
    id: Option<String>,
    model: String,
) -> CmdResult<AppDraft> {
    Ok(state.builder.open(id.as_deref(), &model).await?)
}

/// An app's draft as the builder shows it.
#[tauri::command]
#[specta::specta]
pub async fn app_draft(state: State<'_, AppState>, id: String) -> CmdResult<AppDraft> {
    Ok(state.builder.state(&id).await?)
}

/// Unsaved apps (drafts that were never saved to Utility Apps).
#[tauri::command]
#[specta::specta]
pub async fn list_app_drafts(state: State<'_, AppState>) -> CmdResult<Vec<UtilityApp>> {
    Ok(state.builder.apps().list_drafts().await)
}

/// Saves a file of a draft from Code (a checkpoint follows).
#[tauri::command]
#[specta::specta]
pub async fn save_draft_file(
    state: State<'_, AppState>,
    id: String,
    path: String,
    content: String,
) -> CmdResult<AppDraft> {
    Ok(state.builder.save_file(&id, &path, &content).await?)
}

/// Deletes a file of a draft from Code.
#[tauri::command]
#[specta::specta]
pub async fn delete_draft_file(
    state: State<'_, AppState>,
    id: String,
    path: String,
) -> CmdResult<AppDraft> {
    Ok(state.builder.delete_file(&id, &path).await?)
}

/// Renames a file of a draft from Code.
#[tauri::command]
#[specta::specta]
pub async fn rename_draft_file(
    state: State<'_, AppState>,
    id: String,
    from: String,
    to: String,
) -> CmdResult<AppDraft> {
    Ok(state.builder.rename_file(&id, &from, &to).await?)
}

/// Changes a draft's name, description, icon and category (the builder's details dialog).
#[tauri::command]
#[specta::specta]
pub async fn update_draft_details(
    state: State<'_, AppState>,
    id: String,
    name: String,
    description: String,
    icon: String,
    category: String,
) -> CmdResult<AppDraft> {
    let patch = json!({
        "name": name,
        "description": description,
        "icon": icon,
        "category": category,
    });
    Ok(state.builder.update_manifest(&id, &patch).await?)
}

/// A draft's checkpoints, newest first.
#[tauri::command]
#[specta::specta]
pub async fn app_checkpoints(
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<Vec<AppCheckpoint>> {
    Ok(state.builder.checkpoints(&id).await?)
}

/// Writes a checkpoint back into the draft (the current files are kept as a checkpoint).
#[tauri::command]
#[specta::specta]
pub async fn restore_app_checkpoint(
    state: State<'_, AppState>,
    id: String,
    checkpoint_id: String,
) -> CmdResult<AppDraft> {
    Ok(state.builder.restore(&id, &checkpoint_id).await?)
}

/// **Save to Utility Apps**: the draft becomes the saved app.
#[tauri::command]
#[specta::specta]
pub async fn publish_app_draft(state: State<'_, AppState>, id: String) -> CmdResult<UtilityApp> {
    Ok(state.builder.publish(&id).await?)
}

/// Throws a draft away (a never-saved app goes with its chat and checkpoints).
#[tauri::command]
#[specta::specta]
pub async fn discard_app_draft(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    if let Some(cid) = state.store.builder_chat(&id).await.ok().flatten() {
        state.chat.stop(&cid);
    }
    state.builder.discard(&id).await?;
    Ok(())
}

/// The Preview's answer to `events.appPreviewRequest`. `false`: nobody waits for it.
#[tauri::command]
#[specta::specta]
pub async fn answer_preview_request(
    state: State<'_, AppState>,
    request_id: String,
    report: PreviewReport,
) -> CmdResult<bool> {
    // A screenshot is at most a few MB; anything far larger isn't one.
    if report
        .screenshot
        .as_ref()
        .is_some_and(|s| s.len() > 16 * 1024 * 1024)
    {
        return Ok(state.builder.answer(
            &request_id,
            PreviewReport {
                screenshot: None,
                note: Some("The screenshot was too large to send.".into()),
                ..report
            },
        ));
    }
    Ok(state.builder.answer(&request_id, report))
}
