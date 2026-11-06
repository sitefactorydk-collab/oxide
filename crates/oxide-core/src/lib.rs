//! Domain types shared across Oxide.
//!
//! Plain data only: no I/O and no Tauri. Every type that crosses the IPC boundary derives
//! `serde` + `specta::Type`, so the frontend gets it through the generated bindings.
//! Timestamps are Unix milliseconds (`i64`), exported to TypeScript as `number`.

pub mod attachment;
pub mod chat;
pub mod conversation;
pub mod mcp;
pub mod message;
pub mod model;
pub mod project;
pub mod provider;
pub mod settings;
pub mod tools;

pub use attachment::{
    ATTACHMENT_LIMITS, Attachment, AttachmentKind, AttachmentLimits, AttachmentSource,
};
pub use chat::{
    ChatStreamEvent, ConversationChanged, GenerateOptions, StopReason, StreamEvent, Usage,
};
pub use conversation::{Conversation, SearchHit, StartConversation};
pub use mcp::{
    ChatTools, ImportCandidate, ImportPreview, ImportStatus, McpAnswer, McpAsk, McpCapabilities,
    McpConfigFile, McpContent, McpEra, McpEvent, McpInteraction, McpLogLine, McpLogSource,
    McpPrompt, McpPromptArgument, McpPromptMessage, McpPromptResult, McpResource, McpResourceRead,
    McpResourceTemplate, McpServerConfig, McpServerLists, McpServerState, McpStatus, McpTool,
    McpToolAnnotations, McpToolResult, McpTransport,
};
pub use message::{ContentBlock, Feedback, MediaSource, Message, Rating, Role};
pub use model::{Effort, ModelCapabilities, ModelInfo, ProviderModels, ThinkingStyle};
pub use project::{NewProject, Project, ProjectPatch};
pub use provider::{ProviderConfig, ProviderInput, ProviderKind, ProviderTestResult, TestProvider};
pub use settings::{
    AppSettings, AppearanceSettings, ChatFont, ModelSettings, ProfileSettings, ThemePref,
    ToolSettings, WebSearchProvider, WebSearchSettings,
};
pub use tools::{
    ApprovalDecision, SearchKeyKind, ToolApprovalRequest, ToolUsage, ToolUsageKind, WebSearchKeys,
    WebSearchTest,
};

/// Product name shown in the UI and window titles.
pub const APP_NAME: &str = "Oxide";

/// Unix time in milliseconds.
pub type Timestamp = i64;

/// Version of the core crate (kept in lockstep with the workspace version).
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_name_is_oxide() {
        assert_eq!(APP_NAME, "Oxide");
    }

    #[test]
    fn version_is_semver_like() {
        assert_eq!(version().split('.').count(), 3);
    }
}
