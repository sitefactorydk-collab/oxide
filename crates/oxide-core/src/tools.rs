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
