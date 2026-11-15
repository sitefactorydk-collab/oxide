//! MCP servers: state, lists and logs, the config file and import, sign-in, running tools
//! from the debug panel, attaching prompts and resources, answering server questions, and
//! per-chat tool switches.

use std::collections::BTreeMap;

use oxide_core::{
    Attachment, AttachmentSource, ChatTools, ImportPreview, McpAnswer, McpConfigFile,
    McpInteraction, McpLogLine, McpPromptResult, McpResourceRead, McpServerConfig, McpServerLists,
    McpServerState, McpToolResult,
};
use oxide_mcp::protocol;
use serde::Serialize;
use serde_json::{Map, Value};
use specta::Type;
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use base64::Engine;

use crate::error::{CmdResult, CommandError};
use crate::state::AppState;

/// Trusts a local server's command line (it waited in `needsTrust`) and starts it.
#[tauri::command]
#[specta::specta]
pub async fn mcp_trust_server(state: State<'_, AppState>, name: String) -> CmdResult<()> {
    Ok(state.mcp.trust(&name)?)
}

/// Every configured server and how it's doing, in config order.
#[tauri::command]
#[specta::specta]
pub fn mcp_servers(state: State<'_, AppState>) -> Vec<McpServerState> {
    state.mcp.states()
}

/// A server's tools, resources, templates and prompts (empty until it runs).
#[tauri::command]
#[specta::specta]
pub fn mcp_server_lists(state: State<'_, AppState>, name: String) -> McpServerLists {
    state.mcp.lists(&name).unwrap_or_default()
}

/// The last log lines of a server (stderr, its log messages, Oxide's notes).
#[tauri::command]
#[specta::specta]
pub fn mcp_logs(state: State<'_, AppState>, name: String) -> Vec<McpLogLine> {
    state.mcp.logs(&name)
}

/// One server's entry, for the edit form.
#[tauri::command]
#[specta::specta]
pub fn mcp_server_config(state: State<'_, AppState>, name: String) -> CmdResult<McpServerConfig> {
    Ok(state.mcp.server_config(&name)?)
}

/// Adds a server, or replaces `old_name` (renaming it if the name changed).
#[tauri::command]
#[specta::specta]
pub async fn mcp_save_server(
    state: State<'_, AppState>,
    server: McpServerConfig,
    old_name: Option<String>,
) -> CmdResult<()> {
    Ok(state.mcp.save_server(server, old_name).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn mcp_remove_server(state: State<'_, AppState>, name: String) -> CmdResult<()> {
    Ok(state.mcp.remove_server(&name).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn mcp_set_enabled(
    state: State<'_, AppState>,
    name: String,
    enabled: bool,
) -> CmdResult<()> {
    Ok(state.mcp.set_enabled(&name, enabled).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn mcp_restart(state: State<'_, AppState>, name: String) -> CmdResult<()> {
    Ok(state.mcp.restart(&name)?)
}

/// The config file's path and text, plus why it can't be used (if it can't).
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct McpConfigStatus {
    pub file: McpConfigFile,
    pub error: Option<String>,
}

#[tauri::command]
#[specta::specta]
pub async fn mcp_config(state: State<'_, AppState>) -> CmdResult<McpConfigStatus> {
    Ok(McpConfigStatus {
        file: state.mcp.config_file().await?,
        error: state.mcp.config_error(),
    })
}

/// Saves the config editor's text (it must be valid JSON) and applies it.
#[tauri::command]
#[specta::specta]
pub async fn mcp_save_config(state: State<'_, AppState>, text: String) -> CmdResult<()> {
    Ok(state.mcp.save_config_text(&text).await?)
}

/// Opens `oxide_config.json` in the system's editor for JSON files.
#[tauri::command]
#[specta::specta]
pub async fn mcp_open_config(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    // Make sure the file exists first.
    state.mcp.config_file().await?;
    let path = state.mcp.config_path();
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(|e| {
            tracing::warn!("couldn't open the MCP config: {e}");
            CommandError::Internal("The config file couldn't be opened.".into())
        })
}

/// What "Import from Claude desktop" would add or change (reads its config, read-only).
#[tauri::command]
#[specta::specta]
pub async fn mcp_import_preview(state: State<'_, AppState>) -> CmdResult<ImportPreview> {
    Ok(state.mcp.import_preview().await)
}

/// Copies the chosen servers from Claude desktop's config. Returns how many were copied.
#[tauri::command]
#[specta::specta]
pub async fn mcp_import(state: State<'_, AppState>, names: Vec<String>) -> CmdResult<u32> {
    let n = state.mcp.import(names).await?;
    Ok(u32::try_from(n).unwrap_or(u32::MAX))
}

/// Runs a tool (debug panel). `input` is the arguments as a JSON object in text (not named
/// `arguments`: that name is reserved in the generated JavaScript); `token` names the call
/// in `progress` events.
#[tauri::command]
#[specta::specta]
pub async fn mcp_call_tool(
    state: State<'_, AppState>,
    server: String,
    tool: String,
    input: String,
    token: Option<String>,
) -> CmdResult<McpToolResult> {
    let args: Value = if input.trim().is_empty() {
        Value::Object(Map::new())
    } else {
        serde_json::from_str(&input)
            .map_err(|e| CommandError::Invalid(format!("The arguments aren't valid JSON: {e}")))?
    };
    if !args.is_object() {
        return Err(CommandError::Invalid(
            "The arguments must be a JSON object.".into(),
        ));
    }
    Ok(state.mcp.call_tool(&server, &tool, args, token).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn mcp_read_resource(
    state: State<'_, AppState>,
    server: String,
    uri: String,
) -> CmdResult<McpResourceRead> {
    Ok(state.mcp.read_resource(&server, &uri).await?)
}

fn prompt_args(values: BTreeMap<String, String>) -> Map<String, Value> {
    values
        .into_iter()
        .map(|(k, v)| (k, Value::String(v)))
        .collect()
}

#[tauri::command]
#[specta::specta]
pub async fn mcp_get_prompt(
    state: State<'_, AppState>,
    server: String,
    name: String,
    values: BTreeMap<String, String>,
) -> CmdResult<McpPromptResult> {
    Ok(state
        .mcp
        .get_prompt(&server, &name, &prompt_args(values))
        .await?)
}

/// Runs a prompt and attaches its text to the message being written.
#[tauri::command]
#[specta::specta]
pub async fn mcp_attach_prompt(
    state: State<'_, AppState>,
    server: String,
    name: String,
    values: BTreeMap<String, String>,
) -> CmdResult<Attachment> {
    let result = state
        .mcp
        .get_prompt(&server, &name, &prompt_args(values))
        .await?;
    let text = protocol::prompt_as_text(&result);
    if text.trim().is_empty() {
        return Err(CommandError::Invalid("The prompt came back empty.".into()));
    }
    let file = protocol::resource_file_name(&name, Some("text/markdown"), "prompt");
    let data = base64::engine::general_purpose::STANDARD.encode(text.as_bytes());
    Ok(state
        .attachments
        .prepare(AttachmentSource::Bytes { name: file, data })
        .await?)
}

/// Reads a resource and attaches its contents (text, images, PDFs) as files.
#[tauri::command]
#[specta::specta]
pub async fn mcp_attach_resource(
    state: State<'_, AppState>,
    server: String,
    uri: String,
) -> CmdResult<Vec<Attachment>> {
    let read = state.mcp.read_resource(&server, &uri).await?;
    let files = protocol::resource_files(&read, "resource");
    if files.is_empty() {
        return Err(CommandError::Invalid(
            "The resource has no content Oxide can attach.".into(),
        ));
    }
    let mut out = Vec::new();
    for (name, bytes) in files {
        let data = base64::engine::general_purpose::STANDARD.encode(bytes);
        out.push(
            state
                .attachments
                .prepare(AttachmentSource::Bytes { name, data })
                .await?,
        );
    }
    Ok(out)
}

/// Questions from servers waiting for the user (after a window reload).
#[tauri::command]
#[specta::specta]
pub fn mcp_pending_asks(state: State<'_, AppState>) -> Vec<McpInteraction> {
    state.mcp.pending_asks()
}

/// Answers a server's question. `false` if it was no longer open.
#[tauri::command]
#[specta::specta]
pub fn mcp_answer(state: State<'_, AppState>, id: String, answer: McpAnswer) -> bool {
    state.mcp.answer(&id, answer)
}

/// Signs in to a remote server in the browser, then reconnects it.
#[tauri::command]
#[specta::specta]
pub async fn mcp_sign_in(state: State<'_, AppState>, name: String) -> CmdResult<()> {
    Ok(state.mcp.sign_in(&name).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn mcp_sign_out(state: State<'_, AppState>, name: String) -> CmdResult<()> {
    Ok(state.mcp.sign_out(&name).await?)
}

/// Which servers and tools a chat may use.
#[tauri::command]
#[specta::specta]
pub async fn get_chat_tools(
    state: State<'_, AppState>,
    conversation_id: String,
) -> CmdResult<ChatTools> {
    Ok(state.store.chat_tools(&conversation_id).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn set_chat_tools(
    state: State<'_, AppState>,
    conversation_id: String,
    tools: ChatTools,
) -> CmdResult<()> {
    Ok(state.store.set_chat_tools(&conversation_id, &tools).await?)
}
