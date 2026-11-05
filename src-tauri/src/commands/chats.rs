//! Conversations, messages and search.

use oxide_core::{ContentBlock, Conversation, Message, Role, SearchHit, StartConversation};
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

/// Creates a chat with its first user message; the title comes from the message.
#[tauri::command]
#[specta::specta]
pub async fn start_conversation(
    state: State<'_, AppState>,
    input: StartConversation,
) -> CmdResult<Conversation> {
    if input.text.trim().is_empty() {
        return Err(CommandError::Invalid("the message is empty".into()));
    }
    let (conversation, _) = state.store.start_conversation(input).await?;
    tracing::debug!(id = %conversation.id, "conversation started");
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

/// Appends a user message under `parent_id` (the current leaf of the visible thread).
#[tauri::command]
#[specta::specta]
pub async fn add_user_message(
    state: State<'_, AppState>,
    conversation_id: String,
    parent_id: Option<String>,
    text: String,
) -> CmdResult<Message> {
    let text = text.trim();
    if text.is_empty() {
        return Err(CommandError::Invalid("the message is empty".into()));
    }
    Ok(state
        .store
        .add_message(
            &conversation_id,
            parent_id.as_deref(),
            Role::User,
            vec![ContentBlock::text(text)],
            None,
        )
        .await?)
}

/// Edits a user message by adding a sibling branch with the new text.
#[tauri::command]
#[specta::specta]
pub async fn edit_user_message(
    state: State<'_, AppState>,
    message_id: String,
    text: String,
) -> CmdResult<Message> {
    Ok(state.store.edit_user_message(&message_id, &text).await?)
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
