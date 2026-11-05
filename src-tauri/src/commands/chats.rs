//! Conversations, messages and search.

use oxide_core::{
    ContentBlock, Conversation, GenerateOptions, Message, SearchHit, StartConversation,
};
use tauri::State;

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
    input: StartConversation,
    options: GenerateOptions,
) -> CmdResult<Conversation> {
    if input.text.trim().is_empty() {
        return Err(CommandError::Invalid("the message is empty".into()));
    }
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

#[tauri::command]
#[specta::specta]
pub async fn delete_conversation(state: State<'_, AppState>, id: String) -> CmdResult<()> {
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

/// Sends a user message under `parent_id` (the leaf of the visible thread) and starts the
/// reply. Returns the saved message; the reply streams as `chatStream` events.
#[tauri::command]
#[specta::specta]
pub async fn send_message(
    state: State<'_, AppState>,
    conversation_id: String,
    parent_id: Option<String>,
    text: String,
    options: GenerateOptions,
) -> CmdResult<Message> {
    let text = text.trim();
    if text.is_empty() {
        return Err(CommandError::Invalid("the message is empty".into()));
    }
    Ok(state
        .chat
        .send(
            &conversation_id,
            parent_id.as_deref(),
            vec![ContentBlock::text(text)],
            options,
        )
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
