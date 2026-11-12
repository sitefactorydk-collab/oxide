//! Conversations, messages and search.

use oxide_agent::export::{export_file_name, export_json, export_markdown};
use oxide_agent::path_to;
use oxide_core::{
    Attachment, Compaction, ContentBlock, ContextUsage, Conversation, Feedback, GenerateOptions,
    Message, SearchHit, StartConversation,
};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::error::{CmdResult, CommandError};
use crate::state::AppState;

/// All chats, most recently updated first.
#[tauri::command]
#[specta::specta]
pub async fn list_conversations(state: State<'_, AppState>) -> CmdResult<Vec<Conversation>> {
    Ok(state.store.list_conversations().await?)
}

#[tauri::command]
#[specta::specta]
pub async fn get_conversation(state: State<'_, AppState>, id: String) -> CmdResult<Conversation> {
    Ok(state.store.get_conversation(&id).await?)
}

/// Creates a chat with its first user message (the title comes from the message) and starts
/// the reply. Progress arrives as `chatStream` events.
#[tauri::command]
#[specta::specta]
pub async fn start_conversation(
    state: State<'_, AppState>,
    mut input: StartConversation,
    options: GenerateOptions,
) -> CmdResult<Conversation> {
    if input.text.trim().is_empty() && input.attachments.is_empty() {
        return Err(CommandError::Invalid("the message is empty".into()));
    }
    // Folders are granted with the chat, so the first reply already has its file tools.
    let mut folders = Vec::new();
    for f in std::mem::take(&mut input.folders) {
        folders.push(crate::commands::workspace::resolve_folder(f).await?);
    }
    input.folders = folders;
    // Only files Oxide prepared (in its attachments folder) can be sent.
    state.attachments.blocks(&input.attachments)?;
    let (conversation, first) = state.store.start_conversation(input).await?;
    tracing::debug!(id = %conversation.id, "conversation started");
    // The chat exists either way; a reply that can't start shows up as a retry in the UI.
    if let Err(e) = state.chat.reply(&conversation.id, &first.id, options).await {
        tracing::warn!(id = %conversation.id, "couldn't start the first reply: {e}");
    }
    Ok(conversation)
}

#[tauri::command]
#[specta::specta]
pub async fn rename_conversation(
    state: State<'_, AppState>,
    id: String,
    title: String,
) -> CmdResult<Conversation> {
    Ok(state.store.rename_conversation(&id, &title).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn set_conversation_starred(
    state: State<'_, AppState>,
    id: String,
    starred: bool,
) -> CmdResult<Conversation> {
    Ok(state.store.set_starred(&id, starred).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn set_conversation_model(
    state: State<'_, AppState>,
    id: String,
    model: String,
) -> CmdResult<Conversation> {
    Ok(state.store.set_conversation_model(&id, &model).await?)
}

/// Moves a chat into a project, or out of any project with `projectId: null`.
#[tauri::command]
#[specta::specta]
pub async fn set_conversation_project(
    state: State<'_, AppState>,
    id: String,
    project_id: Option<String>,
) -> CmdResult<Conversation> {
    Ok(state
        .store
        .set_conversation_project(&id, project_id.as_deref())
        .await?)
}

#[tauri::command]
#[specta::specta]
pub async fn delete_conversation(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    // A reply still running (or waiting for approval) in the chat stops with it.
    state.chat.forget(&id);
    state.store.delete_conversation(&id).await?;
    tracing::debug!(%id, "conversation deleted");
    Ok(())
}

/// Every message of a chat, all branches, parents before children.
#[tauri::command]
#[specta::specta]
pub async fn list_messages(
    state: State<'_, AppState>,
    conversation_id: String,
) -> CmdResult<Vec<Message>> {
    Ok(state.store.list_messages(&conversation_id).await?)
}

/// Sends a user message under `parent_id` (the leaf of the visible thread) with optional
/// attachments, and starts the reply. Returns the saved message; the reply streams as
/// `chatStream` events.
#[tauri::command]
#[specta::specta]
pub async fn send_message(
    state: State<'_, AppState>,
    conversation_id: String,
    parent_id: Option<String>,
    text: String,
    attachments: Vec<Attachment>,
    options: GenerateOptions,
) -> CmdResult<Message> {
    let text = text.trim();
    // Files first, then the question (models answer better that way).
    let mut content = state.attachments.blocks(&attachments)?;
    if !text.is_empty() {
        content.push(ContentBlock::text(text));
    }
    if content.is_empty() {
        return Err(CommandError::Invalid("the message is empty".into()));
    }
    Ok(state
        .chat
        .send(&conversation_id, parent_id.as_deref(), content, options)
        .await?)
}

/// Edits a user message as a new branch (a sibling with the new text) and answers it.
#[tauri::command]
#[specta::specta]
pub async fn edit_and_resend(
    state: State<'_, AppState>,
    message_id: String,
    text: String,
    options: GenerateOptions,
) -> CmdResult<Message> {
    Ok(state
        .chat
        .edit_and_resend(&message_id, &text, options)
        .await?)
}

/// Writes a new reply: for an assistant message, a sibling of it; for a user message, an
/// answer to it (retry after a failure). Returns the id the reply will be saved under.
#[tauri::command]
#[specta::specta]
pub async fn regenerate(
    state: State<'_, AppState>,
    message_id: String,
    options: GenerateOptions,
) -> CmdResult<String> {
    Ok(state.chat.regenerate(&message_id, options).await?)
}

/// How full a chat's context is for the thread ending at `leaf_id` (the composer's meter).
#[tauri::command]
#[specta::specta]
pub async fn chat_context(
    state: State<'_, AppState>,
    conversation_id: String,
    leaf_id: Option<String>,
) -> CmdResult<ContextUsage> {
    Ok(state
        .chat
        .context_usage(&conversation_id, leaf_id.as_deref())
        .await?)
}

/// The summaries that stand in for the start of a chat (the transcript's dividers).
#[tauri::command]
#[specta::specta]
pub async fn chat_compactions(
    state: State<'_, AppState>,
    conversation_id: String,
) -> CmdResult<Vec<Compaction>> {
    Ok(state.chat.compactions(&conversation_id).await?)
}

/// **Compact now**: summarizes the thread ending at `leaf_id`; the next reply starts from the
/// summary.
#[tauri::command]
#[specta::specta]
pub async fn compact_chat(
    state: State<'_, AppState>,
    conversation_id: String,
    leaf_id: String,
) -> CmdResult<Compaction> {
    Ok(state.chat.compact_now(&conversation_id, &leaf_id).await?)
}

/// Stops the reply being written in a chat, keeping what was written. Returns whether a reply
/// was running.
#[tauri::command]
#[specta::specta]
pub async fn stop_generation(
    state: State<'_, AppState>,
    conversation_id: String,
) -> CmdResult<bool> {
    Ok(state.chat.stop(&conversation_id))
}

/// Full-text search over chat titles and message text.
#[tauri::command]
#[specta::specta]
pub async fn search_conversations(
    state: State<'_, AppState>,
    query: String,
    limit: u32,
) -> CmdResult<Vec<SearchHit>> {
    Ok(state.store.search(&query, limit).await?)
}

/// Rates a reply (or clears the rating with `None`). Ratings stay on this computer.
#[tauri::command]
#[specta::specta]
pub async fn set_message_feedback(
    state: State<'_, AppState>,
    message_id: String,
    feedback: Option<Feedback>,
) -> CmdResult<Message> {
    Ok(state.store.set_feedback(&message_id, feedback).await?)
}

/// Export formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ExportFormat {
    /// The visible thread as Markdown.
    Markdown,
    /// Every message and branch as JSON.
    Json,
}

/// Saves a chat to a file the user picks. Markdown exports the thread ending at `leaf_id`
/// (the newest message when `None`); JSON exports every branch. Returns the saved path, or
/// `None` if the user cancelled.
#[tauri::command]
#[specta::specta]
pub async fn export_conversation(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    leaf_id: Option<String>,
    format: ExportFormat,
) -> CmdResult<Option<String>> {
    let conversation = state.store.get_conversation(&id).await?;
    let messages = state.store.list_messages(&id).await?;
    let now = oxide_store::now_ms();
    let (text, ext, label) = match format {
        ExportFormat::Markdown => {
            let leaf = leaf_id
                .filter(|l| messages.iter().any(|m| &m.id == l))
                .or_else(|| messages.last().map(|m| m.id.clone()));
            let thread = leaf
                .and_then(|l| path_to(&messages, &l))
                .unwrap_or_default();
            (
                export_markdown(&conversation, &thread, now),
                "md",
                "Markdown",
            )
        }
        ExportFormat::Json => (
            export_json(&conversation, &messages, now).map_err(|e| {
                tracing::error!("export failed: {e}");
                CommandError::Internal("The chat couldn't be exported.".into())
            })?,
            "json",
            "JSON",
        ),
    };

    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .set_title("Export chat")
        .set_file_name(export_file_name(&conversation.title, ext))
        .add_filter(label, &[ext])
        .save_file(move |path| {
            let _ = tx.send(path);
        });
    let picked = rx
        .await
        .map_err(|_| CommandError::Internal("the save dialog closed unexpectedly".into()))?;
    let Some(path) = picked.and_then(|p| p.into_path().ok()) else {
        return Ok(None);
    };
    tokio::fs::write(&path, text).await.map_err(|e| {
        tracing::warn!("couldn't write the export: {e}");
        CommandError::Invalid(format!("The file couldn't be saved: {e}"))
    })?;
    Ok(Some(path.to_string_lossy().into_owned()))
}

/// Creates a 50-message demo chat (code, math, tables, images) on `model`, for checking
/// rendering and scrolling. Nothing is sent to the model.
#[tauri::command]
#[specta::specta]
pub async fn create_demo_conversation(
    state: State<'_, AppState>,
    model: String,
) -> CmdResult<Conversation> {
    Ok(
        oxide_agent::demo::create_demo_conversation(&state.store, &state.attachments, &model)
            .await?,
    )
}
