//! Tool calls: approvals, the per-chat log of what a chat used, and built-in tool names.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::Timestamp;

/// Oxide's built-in web search tool.
pub const WEB_SEARCH: &str = "web_search";
/// Oxide's built-in web page reader.
pub const WEB_FETCH: &str = "web_fetch";
/// Oxide's built-in search over a project's knowledge files.
pub const PROJECT_SEARCH: &str = "project_search";
/// `server` of tools the model provider runs itself (Anthropic's web search).
pub const PROVIDER_SERVER: &str = "anthropic";

/// A tool call that waits for the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ToolApprovalRequest {
    pub conversation_id: String,
    /// The reply the call belongs to.
    pub message_id: String,
    pub tool_use_id: String,
    /// MCP server; `None` for built-in tools.
    pub server: Option<String>,
    pub tool: String,
    pub title: Option<String>,
    pub description: Option<String>,
    /// The arguments as pretty-printed JSON.
    pub input_json: String,
    /// The server says the tool only reads.
    pub read_only: bool,
    /// The server says the tool may destroy data.
    pub destructive: bool,
    /// For `save_skill`: what would be written (the card shows it; it never offers "Allow
    /// always").
    pub skill: Option<crate::SkillSavePreview>,
    /// For computer use: the card that starts it in a chat, or one for an app.
    #[serde(default)]
    pub computer: Option<Box<crate::computer::ComputerApproval>>,
    pub requested_at: Timestamp,
}

/// The user's answer to a [`ToolApprovalRequest`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ApprovalDecision {
    AllowOnce,
    /// Allow this tool for the rest of the chat (saved).
    AllowChat,
    Deny,
}

/// What kind of thing a chat used (the "Used in this chat" list).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ToolUsageKind {
    WebSearch,
    WebFetch,
    /// An MCP server's tool.
    Connector,
    /// A tool the provider ran (Anthropic web search).
    ProviderTool,
    /// A search of the project's knowledge.
    ProjectSearch,
    /// An artifact made or changed (the label is its title).
    Artifact,
    /// A skill's instructions or files were loaded (the label is the skill's name).
    Skill,
    /// A file or folder in a granted folder was listed, read, written, moved or deleted (the
    /// label is the path).
    File,
    /// A shell command ran (the label is the command).
    Shell,
    /// The chat's task list was set (the label is the current step).
    Tasks,
    /// Python code ran (the label is the script's first line).
    Python,
    /// The Docker sandbox: a command, its file tools or a copy in or out (the label is the
    /// command or the path).
    Sandbox,
    /// The browser agent: a page opened, read or clicked, or a whole browsing task (the
    /// label is the URL or the goal).
    Browser,
    /// The user's Chrome through Oxide's extension: a tab opened, read, clicked or typed in
    /// (the label is the URL or what was clicked or typed).
    Chrome,
    /// Computer use: the screen seen, the mouse and keyboard used (the label is the action
    /// or the app).
    Computer,
    /// `generate_image`, `generate_video`, `generate_music` (Phase 19).
    Generation,
}

/// One tool call, logged per chat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ToolUsage {
    pub id: String,
    pub conversation_id: String,
    pub message_id: String,
    pub tool_use_id: String,
    pub kind: ToolUsageKind,
    pub server: Option<String>,
    pub tool: String,
    /// A short label: the search query, the URL, a file path.
    pub label: Option<String>,
    pub is_error: bool,
    pub created_at: Timestamp,
}

/// Masked hints of the web search API keys in the keychain.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WebSearchKeys {
    pub brave: Option<String>,
    pub tavily: Option<String>,
}

/// A web search service whose key lives in the keychain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum SearchKeyKind {
    Brave,
    Tavily,
}

impl SearchKeyKind {
    /// The keychain entry name.
    pub fn secret_name(self) -> &'static str {
        match self {
            SearchKeyKind::Brave => "websearch:brave",
            SearchKeyKind::Tavily => "websearch:tavily",
        }
    }
}

/// The result of "Test" in Settings → Web search.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WebSearchTest {
    pub ok: bool,
    pub message: String,
    /// Titles of the first results.
    pub sample: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decisions_are_camel_case() {
        let json = serde_json::to_string(&ApprovalDecision::AllowChat).expect("serializes");
        assert_eq!(json, "\"allowChat\"");
        assert_eq!(SearchKeyKind::Brave.secret_name(), "websearch:brave");
    }
}
