//! Domain types shared across Oxide.
//!
//! Plain data only: no I/O and no Tauri. Every type that crosses the IPC boundary derives
//! `serde` + `specta::Type`, so the frontend gets it through the generated bindings.
//! Timestamps are Unix milliseconds (`i64`), exported to TypeScript as `number`.

pub mod app_builder;
pub mod artifact;
pub mod attachment;
pub mod browser;
pub mod chat;
pub mod chrome;
pub mod computer;
pub mod context;
pub mod conversation;
pub mod data;
pub mod desktop;
pub mod extension;
pub mod generation;
pub mod mcp;
pub mod memory;
pub mod message;
pub mod model;
pub mod permission;
pub mod project;
pub mod provider;
pub mod python;
pub mod redact;
pub mod sandbox;
pub mod schedule;
pub mod settings;
pub mod tools;
pub mod usage;
pub mod utility_app;
pub mod workspace;

pub use app_builder::{
    APP_BUILDER_TOOLS, AppCheckpoint, AppDraft, CheckpointKind, MAX_APP_CHECKPOINTS,
    PreviewConsoleLine, PreviewReport, PreviewRequestKind, checkpoint_label,
};
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
pub use computer::{
    ComputerActivity, ComputerActivityState, ComputerApp, ComputerApproval, ComputerApprovalKind,
    ComputerDisplay, ComputerModelSettings, ComputerSettings, ComputerStatus, ComputerTuning,
    PointerCalibration,
};
pub use context::{
    Compaction, CompactionTrigger, ContextSettings, ContextUsage, ContextWindowOverride,
    WindowSource,
};
pub use conversation::{Conversation, SearchHit, StartConversation};
pub use data::{
    DELETE_ALL_CONFIRMATION, DataDeleted, DataExport, DataLocations, UpdateInfo, UpdatePhase,
    UpdateSettings, UpdateStatus,
};
pub use desktop::DesktopAction;
pub use extension::{
    Extension, ExtensionField, ExtensionFieldType, ExtensionInfo, ExtensionItem, ExtensionPreview,
    ExtensionServerType, ExtensionStatus, ExtensionValue, LOAD_SKILL, SAVE_SKILL, Skill,
    SkillDetail, SkillFileChange, SkillInput, SkillSavePreview,
};
pub use generation::{
    GENERATE_IMAGE, GENERATE_MUSIC, GENERATE_VIDEO, GENERATION_SWITCH, GenerationEndpoint,
    GenerationKind, GenerationSettings, GenerationTest, ToolProgress,
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
pub use permission::{AutoMark, MAX_ALWAYS_ASK, PermissionMode, always_ask_matches};
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
pub use schedule::{
    ApprovalMode, CronSpec, FixedZone, LocalZone, MAX_RUN_OUTPUT_CHARS, MAX_RUNS_KEPT, MAX_TASKS,
    MISSED_AFTER_MS, MissedRunPolicy, RunStatus, RunTrigger, ScheduleChange, ScheduledTask,
    TaskInput, TaskRun, TaskSchedule,
};
pub use settings::{
    AppSettings, AppearanceSettings, ChatFont, DesktopSettings, DictationSettings, MemorySettings,
    ModelImagesOverride, ModelSettings, ProfileSettings, ProjectSettings, ShellSettings, ThemePref,
    ToolSettings, ToolSteps, UiLanguage, WebSearchProvider, WebSearchSettings,
};
pub use tools::{
    ApprovalDecision, SearchKeyKind, ToolApprovalRequest, ToolUsage, ToolUsageKind, WebSearchKeys,
    WebSearchTest,
};
pub use usage::{UsageByModel, UsageByProvider, UsageDay, UsageRecord, UsageReport, UsageTotals};
pub use utility_app::{
    APP_FILE_EXT, APP_MANIFEST, AppBundle, AppCallError, AppCapability, AppErrorCode, AppFile,
    AppGenerationOverride, AppImportPreview, AppManifest, AppModels, AppPrefs, AppPrefsView,
    MAX_APP_BYTES, MAX_APP_FILE_BYTES, MAX_APP_FILES, MAX_APP_STORAGE_BYTES, UtilityApp,
    app_secret_name, app_secret_prefix, app_slug, valid_app_id, valid_app_path,
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
