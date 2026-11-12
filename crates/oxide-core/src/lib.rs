//! Domain types shared across Oxide.
//!
//! Plain data only: no I/O and no Tauri. Every type that crosses the IPC boundary derives
//! `serde` + `specta::Type`, so the frontend gets it through the generated bindings.
//! Timestamps are Unix milliseconds (`i64`), exported to TypeScript as `number`.

pub mod artifact;
pub mod attachment;
pub mod browser;
pub mod chat;
pub mod chrome;
pub mod context;
pub mod conversation;
pub mod desktop;
pub mod extension;
pub mod mcp;
pub mod memory;
pub mod message;
pub mod model;
pub mod project;
pub mod provider;
pub mod python;
pub mod sandbox;
pub mod settings;
pub mod tools;
pub mod workspace;

pub use artifact::{
    ARTIFACT_TOOL, Artifact, ArtifactDetail, ArtifactKind, ArtifactSummary, ArtifactVersion,
};
pub use attachment::{
    ATTACHMENT_LIMITS, Attachment, AttachmentKind, AttachmentLimits, AttachmentSource,
};
pub use browser::{
    BrowserFrame, BrowserSettings, BrowserSource, BrowserStatus, BrowserStep, InstalledBrowser,
};
pub use chat::{
    ChatStreamEvent, ConversationChanged, GenerateOptions, StopReason, StreamEvent, Usage,
};
pub use chrome::{ChromeSettings, ChromeStatus, PairedBrowser, PairingCode};
pub use context::{
    Compaction, CompactionTrigger, ContextSettings, ContextUsage, ContextWindowOverride,
    WindowSource,
};
pub use conversation::{Conversation, SearchHit, StartConversation};
pub use desktop::DesktopAction;
pub use extension::{
    Extension, ExtensionField, ExtensionFieldType, ExtensionInfo, ExtensionItem, ExtensionPreview,
    ExtensionServerType, ExtensionStatus, ExtensionValue, LOAD_SKILL, Skill, SkillDetail,
    SkillInput,
};
pub use mcp::{
    ChatTools, ImportCandidate, ImportPreview, ImportStatus, McpAnswer, McpAsk, McpCapabilities,
    McpConfigFile, McpContent, McpEra, McpEvent, McpInteraction, McpLogLine, McpLogSource,
    McpPrompt, McpPromptArgument, McpPromptMessage, McpPromptResult, McpResource, McpResourceRead,
    McpResourceTemplate, McpServerConfig, McpServerLists, McpServerState, McpStatus, McpTool,
    McpToolAnnotations, McpToolResult, McpTransport,
};
pub use memory::{MemoryItem, NewMemory};
pub use message::{ContentBlock, Feedback, MediaSource, Message, Rating, Role};
pub use model::{Effort, ModelCapabilities, ModelInfo, ProviderModels, ThinkingStyle};
pub use project::{
    KnowledgeFile, KnowledgeHit, KnowledgeKind, KnowledgeMode, KnowledgeText, NewProject, Project,
    ProjectKnowledge, ProjectPatch,
};
pub use provider::{ProviderConfig, ProviderInput, ProviderKind, ProviderTestResult, TestProvider};
pub use python::{
    PYTHON_PACKAGES, PYTHON_VERSION, PythonPackage, PythonSettings, PythonState, PythonStatus,
    PythonStep, RUN_PYTHON,
};
pub use sandbox::{
    ChatSandbox, DockerState, DockerStatus, SandboxChange, SandboxCommand, SandboxFile,
    SandboxImage, SandboxListing, SandboxSettings, SandboxState, SandboxStorage,
};
pub use settings::{
    AppSettings, AppearanceSettings, ChatFont, DesktopSettings, MemorySettings, ModelSettings,
    ProfileSettings, ProjectSettings, ShellSettings, ThemePref, ToolSettings, WebSearchProvider,
    WebSearchSettings,
};
pub use tools::{
    ApprovalDecision, SearchKeyKind, ToolApprovalRequest, ToolUsage, ToolUsageKind, WebSearchKeys,
    WebSearchTest,
};
pub use workspace::{ChatFolder, ChatWorkspace, TaskItem, TaskList, TaskStatus};

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
