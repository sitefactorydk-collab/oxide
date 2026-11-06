//! The tool loop: approvals, what a chat used, web search settings.

use oxide_core::{
    ApprovalDecision, SearchKeyKind, ToolApprovalRequest, ToolUsage, WebSearchKeys,
    WebSearchSettings, WebSearchTest,
};
use tauri::State;

use crate::error::CmdResult;
use crate::state::AppState;

/// Answers a tool call that waits for approval. Returns whether it was still waiting.
#[tauri::command]
#[specta::specta]
pub async fn answer_tool_approval(
    state: State<'_, AppState>,
    conversation_id: String,
    tool_use_id: String,
    decision: ApprovalDecision,
) -> CmdResult<bool> {
    Ok(state
        .chat
        .answer_approval(&conversation_id, &tool_use_id, decision))
}

/// Tool calls waiting for approval in every chat (after a window reload).
#[tauri::command]
#[specta::specta]
pub async fn pending_tool_approvals(
    state: State<'_, AppState>,
) -> CmdResult<Vec<ToolApprovalRequest>> {
    Ok(state.chat.pending_approvals())
}

/// Every tool call a chat made, oldest first ("Used in this chat").
#[tauri::command]
#[specta::specta]
pub async fn tool_usage(
    state: State<'_, AppState>,
    conversation_id: String,
) -> CmdResult<Vec<ToolUsage>> {
    Ok(state.store.tool_usage(&conversation_id).await?)
}

/// Tools allowed for the rest of a chat (`server/tool`, `builtin/<name>`).
#[tauri::command]
#[specta::specta]
pub async fn chat_tool_grants(
    state: State<'_, AppState>,
    conversation_id: String,
) -> CmdResult<Vec<String>> {
    Ok(state.store.chat_tool_grants(&conversation_id).await?)
}

/// Withdraws "Allow always" for one tool, or every tool of the chat when `tool_key` is null.
#[tauri::command]
#[specta::specta]
pub async fn revoke_chat_tool_grant(
    state: State<'_, AppState>,
    conversation_id: String,
    tool_key: Option<String>,
) -> CmdResult<Vec<String>> {
    state
        .store
        .revoke_chat_tools(&conversation_id, tool_key.as_deref())
        .await?;
    Ok(state.store.chat_tool_grants(&conversation_id).await?)
}

/// The window's time zone, for the date and time models are told.
#[tauri::command]
#[specta::specta]
pub async fn set_client_clock(
    state: State<'_, AppState>,
    utc_offset_minutes: i32,
    time_zone: Option<String>,
) -> CmdResult<()> {
    state.chat.set_client_clock(utc_offset_minutes, time_zone);
    Ok(())
}

/// Masked hints of the stored web search keys.
#[tauri::command]
#[specta::specta]
pub async fn web_search_keys(state: State<'_, AppState>) -> CmdResult<WebSearchKeys> {
    Ok(state.chat.tools().web.keys().await)
}

/// Stores (or with `null`, removes) a web search API key in the keychain.
#[tauri::command]
#[specta::specta]
pub async fn set_web_search_key(
    state: State<'_, AppState>,
    service: SearchKeyKind,
    key: Option<String>,
) -> CmdResult<WebSearchKeys> {
    let web = &state.chat.tools().web;
    web.set_key(service, key)
        .await
        .map_err(crate::error::CommandError::Keychain)?;
    Ok(web.keys().await)
}

/// Runs a test search with `settings` (not saved) and an optional typed key.
#[tauri::command]
#[specta::specta]
pub async fn test_web_search(
    state: State<'_, AppState>,
    settings: WebSearchSettings,
    key: Option<String>,
) -> CmdResult<WebSearchTest> {
    let web = &state.chat.tools().web;
    Ok(
        match web.search(&settings, "Oxide desktop app", 3, key).await {
            Ok(hits) => WebSearchTest {
                ok: true,
                message: format!(
                    "{} result{} for a test search.",
                    hits.len(),
                    if hits.len() == 1 { "" } else { "s" }
                ),
                sample: hits.into_iter().map(|h| h.title).collect(),
            },
            Err(e) => WebSearchTest {
                ok: false,
                message: e,
                sample: Vec::new(),
            },
        },
    )
}
