//! MCP (Model Context Protocol) types shown in the UI: configured servers, their state, and
//! what they offer (tools, resources, prompts), plus requests a server makes of the user.
//!
//! JSON values that are passed through unchanged (input schemas, structured tool output,
//! elicitation forms) travel as JSON **strings**, like `ContentBlock::ToolUse.inputJson`, so
//! the IPC types stay free of untyped values.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::Timestamp;

/// How Oxide talks to a server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum McpTransport {
    /// A local process that speaks JSON-RPC on stdin/stdout.
    Stdio,
    /// A remote (or local) Streamable HTTP endpoint.
    Http,
}

/// One server entry, as edited in Settings. Mirrors an `mcpServers` entry in
/// `oxide_config.json` (Claude desktop's schema), plus the Oxide-only `enabled` flag that
/// lives under the file's top-level `oxide` object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct McpServerConfig {
    pub name: String,
    pub transport: McpTransport,
    /// Stdio: the program to run (`npx`, `uvx`, a path).
    pub command: String,
    pub args: Vec<String>,
    /// Stdio: extra environment variables (merged over Oxide's own environment).
    pub env: BTreeMap<String, String>,
    /// HTTP: the MCP endpoint URL.
    pub url: String,
    /// HTTP: extra request headers (e.g. a static `Authorization`).
    pub headers: BTreeMap<String, String>,
    pub enabled: bool,
}

/// Where a server is in its life cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum McpStatus {
    /// Turned off in the config.
    Disabled,
    /// Spawning, connecting or negotiating.
    Starting,
    Running,
    /// Failed; it is retried with backoff until the attempts run out.
    Error,
    /// The remote server wants the user to sign in (OAuth).
    NeedsAuth,
    /// Stopped by Oxide (e.g. while quitting).
    Stopped,
    /// A local command Oxide hasn't run before (or whose command line changed): it waits
    /// until the user trusts it (Phase 18).
    NeedsTrust,
}

/// Which generation of the protocol a server speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum McpEra {
    /// 2026-07-28 and later: stateless, metadata on every request.
    Modern,
    /// 2025-11-25 and earlier: `initialize` handshake and sessions.
    Legacy,
}

/// What a running server says it supports.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct McpCapabilities {
    pub tools: bool,
    pub resources: bool,
    pub prompts: bool,
    pub logging: bool,
    /// The server announces changes to its lists.
    pub list_changed: bool,
}

/// A configured server and how it is doing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct McpServerState {
    pub name: String,
    pub transport: McpTransport,
    /// Command line or URL, for display.
    pub summary: String,
    pub status: McpStatus,
    /// Why it failed, written for users.
    pub error: Option<String>,
    pub era: Option<McpEra>,
    pub protocol_version: Option<String>,
    /// The server's own name and version (self-reported).
    pub server_name: Option<String>,
    pub server_version: Option<String>,
    pub instructions: Option<String>,
    pub capabilities: McpCapabilities,
    pub tools: u32,
    pub resources: u32,
    pub prompts: u32,
    /// Restarts after crashes since Oxide started it.
    pub restarts: u32,
    /// Has a stored OAuth sign-in.
    pub signed_in: bool,
    /// The desktop extension this server comes from (its display name). Such servers are
    /// managed in Settings → Extensions, not edited as config entries.
    pub extension: Option<String>,
    /// With [`McpStatus::NeedsTrust`]: an earlier command line of this server was trusted
    /// and it has changed since.
    pub trust_changed: bool,
    pub updated_at: Timestamp,
}

/// Hints a server gives about a tool (all advisory).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct McpToolAnnotations {
    pub title: Option<String>,
    pub read_only_hint: Option<bool>,
    pub destructive_hint: Option<bool>,
    pub idempotent_hint: Option<bool>,
    pub open_world_hint: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct McpTool {
    pub server: String,
    pub name: String,
    pub title: Option<String>,
    pub description: Option<String>,
    /// JSON Schema of the arguments, as a JSON string.
    pub input_schema: String,
    pub annotations: McpToolAnnotations,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct McpResource {
    pub server: String,
    pub uri: String,
    pub name: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub mime_type: Option<String>,
    pub size: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct McpResourceTemplate {
    pub server: String,
    pub uri_template: String,
    pub name: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct McpPromptArgument {
    pub name: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct McpPrompt {
    pub server: String,
    pub name: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub arguments: Vec<McpPromptArgument>,
}

/// Everything a server currently offers.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct McpServerLists {
    pub tools: Vec<McpTool>,
    pub resources: Vec<McpResource>,
    pub templates: Vec<McpResourceTemplate>,
    pub prompts: Vec<McpPrompt>,
}

/// One piece of a tool result, prompt message or resource.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum McpContent {
    Text {
        text: String,
    },
    /// Base64 data.
    #[serde(rename_all = "camelCase")]
    Image {
        data: String,
        mime_type: String,
    },
    /// Base64 data.
    #[serde(rename_all = "camelCase")]
    Audio {
        data: String,
        mime_type: String,
    },
    /// A link to a resource the client may read.
    #[serde(rename_all = "camelCase")]
    ResourceLink {
        uri: String,
        name: Option<String>,
        mime_type: Option<String>,
        description: Option<String>,
    },
    /// An embedded resource: `text`, or `blob` (base64).
    #[serde(rename_all = "camelCase")]
    Resource {
        uri: String,
        mime_type: Option<String>,
        text: Option<String>,
        blob: Option<String>,
    },
}

/// The result of `tools/call`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct McpToolResult {
    pub content: Vec<McpContent>,
    /// `structuredContent` as a JSON string.
    pub structured: Option<String>,
    /// The tool ran but failed (the content explains why).
    pub is_error: bool,
    pub duration_ms: u64,
}

/// The result of `resources/read`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct McpResourceRead {
    pub contents: Vec<McpContent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct McpPromptMessage {
    /// `user` or `assistant`.
    pub role: String,
    pub content: McpContent,
}

/// The result of `prompts/get`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct McpPromptResult {
    pub description: Option<String>,
    pub messages: Vec<McpPromptMessage>,
}

/// Where a log line came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum McpLogSource {
    /// The process's standard error.
    Stderr,
    /// A `notifications/message` log message.
    Server,
    /// Oxide's own notes (connected, restarted, errors).
    Oxide,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct McpLogLine {
    pub at: Timestamp,
    pub source: McpLogSource,
    /// Log level for server messages (`info`, `error`, …).
    pub level: Option<String>,
    pub text: String,
}

/// Something a server asks of the user while handling a request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum McpAsk {
    /// Elicitation: fill in a form (`schema` is the requested JSON Schema as a string), or
    /// open a URL (`url` mode).
    #[serde(rename_all = "camelCase")]
    Elicitation {
        message: String,
        schema: Option<String>,
        url: Option<String>,
    },
    /// Sampling: the server wants a model to answer. Shown for approval first.
    #[serde(rename_all = "camelCase")]
    Sampling {
        system_prompt: Option<String>,
        /// The conversation the server wants answered, as plain text per turn.
        messages: Vec<McpPromptMessage>,
        max_tokens: u32,
        /// Model names the server would like, most preferred first.
        model_hints: Vec<String>,
    },
}

/// A pending [`McpAsk`] with its id and server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct McpInteraction {
    pub id: String,
    pub server: String,
    pub ask: McpAsk,
    pub created_at: Timestamp,
}

/// The user's answer to an [`McpInteraction`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum McpAnswer {
    /// `action` is `accept`, `decline` or `cancel`; `content` is the form data as a JSON
    /// string (accept only).
    Elicitation {
        action: String,
        content: Option<String>,
    },
    /// Run the sampling request with `model` (`None`: the default model), or refuse it.
    Sampling {
        approved: bool,
        model: Option<String>,
    },
}

/// Something changed in the MCP layer (`events.mcpEvent` in TypeScript).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum McpEvent {
    /// A server's state changed (status, counts, error).
    Server { state: McpServerState },
    /// A server was removed from the config.
    Removed { name: String },
    /// A server's tools, resources or prompts changed.
    Lists { server: String },
    /// A new log line.
    Log { server: String, line: McpLogLine },
    /// Progress of a running call (`token` is the call id the UI chose).
    #[serde(rename_all = "camelCase")]
    Progress {
        token: String,
        progress: f64,
        total: Option<f64>,
        message: Option<String>,
    },
    /// A server needs the user (see [`McpInteraction`]).
    Ask { interaction: McpInteraction },
    /// An ask was answered, timed out or withdrawn.
    AskClosed { id: String },
}

/// Per-chat tool switches. Everything is on unless listed here.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", default)]
pub struct ChatTools {
    /// Servers whose tools are off in this chat.
    pub disabled_servers: Vec<String>,
    /// Single tools that are off, as `server/tool`.
    pub disabled_tools: Vec<String>,
    /// Built-in tools that are off (`web_search`, `web_fetch`).
    pub disabled_builtins: Vec<String>,
}

impl ChatTools {
    /// Whether the built-in tool `name` may be used in this chat.
    pub fn allows_builtin(&self, name: &str) -> bool {
        !self.disabled_builtins.iter().any(|b| b == name)
    }

    /// Whether `tool` of `server` may be used in this chat.
    pub fn allows(&self, server: &str, tool: &str) -> bool {
        !self.disabled_servers.iter().any(|s| s == server)
            && !self
                .disabled_tools
                .iter()
                .any(|t| t.split_once('/') == Some((server, tool)))
    }
}

/// How an entry in Claude desktop's config compares to Oxide's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ImportStatus {
    /// Oxide has no server of that name.
    New,
    /// Oxide has the same entry already.
    Same,
    /// Oxide has a different entry under that name; importing replaces it.
    Changed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportCandidate {
    pub config: McpServerConfig,
    pub status: ImportStatus,
}

/// What "Import from Claude desktop" would do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    /// The file that was read, or the first place looked if none was found.
    pub path: String,
    pub found: bool,
    pub candidates: Vec<ImportCandidate>,
}

/// The raw config file, for the "Edit config" editor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct McpConfigFile {
    pub path: String,
    pub text: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_tools_filter_servers_and_single_tools() {
        let t = ChatTools {
            disabled_servers: vec!["fs".into()],
            disabled_tools: vec!["web/fetch".into()],
            disabled_builtins: vec!["web_search".into()],
        };
        assert!(!t.allows("fs", "read_file"));
        assert!(!t.allows("web", "fetch"));
        assert!(!t.allows_builtin("web_search"));
        assert!(t.allows_builtin("web_fetch"));
        assert!(t.allows("web", "search"));
        assert!(ChatTools::default().allows("fs", "x"));
    }

    #[test]
    fn content_is_tagged_by_type() {
        let c = McpContent::Image {
            data: "AA==".into(),
            mime_type: "image/png".into(),
        };
        let v = serde_json::to_value(&c).expect("serializes");
        assert_eq!(
            v,
            serde_json::json!({"type": "image", "data": "AA==", "mimeType": "image/png"})
        );
    }
}
