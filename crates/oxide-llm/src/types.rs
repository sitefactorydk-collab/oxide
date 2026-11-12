//! Provider-neutral request and event types.

use futures_util::stream::BoxStream;
use oxide_core::{ContentBlock, Effort, Role, StopReason, Usage};
use serde_json::Value;

use crate::LlmError;

/// One turn of the conversation sent to a model.
#[derive(Debug, Clone, PartialEq)]
pub struct ChatMessage {
    pub role: Role,
    pub content: Vec<ContentBlock>,
}

impl ChatMessage {
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: vec![ContentBlock::text(text)],
        }
    }
}

/// A tool the model may call. `input_schema` is a JSON Schema object.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

/// Extended thinking for this request.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Thinking {
    /// Send nothing; the model's default applies.
    #[default]
    Default,
    /// Switch thinking off explicitly (models with adaptive thinking may think by default).
    Off,
    /// Think with this many tokens of budget.
    Budget(u32),
    /// Let the model decide how much to think.
    Adaptive,
}

impl Thinking {
    pub fn is_on(self) -> bool {
        matches!(self, Thinking::Budget(_) | Thinking::Adaptive)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChatRequest {
    /// The model's id at the provider.
    pub model: String,
    pub system: Option<String>,
    /// Long, stable context sent ahead of `system` (project knowledge). Anthropic gets it as
    /// its own system block with the prompt-cache breakpoint, so it stays cached while the
    /// rest of the system prompt (which carries the time) changes. Others get it joined.
    pub system_context: Option<String>,
    pub messages: Vec<ChatMessage>,
    pub tools: Vec<ToolSpec>,
    /// Tools the provider runs itself, as the provider's JSON definitions (Anthropic's
    /// `web_search_20250305`). Ignored by OpenAI-compatible servers.
    pub server_tools: Vec<Value>,
    /// Output token limit. Required by Anthropic (a default is used when `None`); omitted for
    /// OpenAI-compatible servers when `None`, so their own default applies.
    pub max_tokens: Option<u32>,
    pub thinking: Thinking,
    /// Mark prompt-cache breakpoints (Anthropic): system prompt, tools and the latest turns.
    pub cache: bool,
    /// Effort level (Anthropic `output_config.effort`, OpenAI `reasoning_effort`). `None`
    /// leaves the model's default.
    pub effort: Option<Effort>,
    /// The model can see images. When it can't, images are replaced by a short note.
    pub images: bool,
    /// The model reads PDFs natively (Anthropic). Otherwise their extracted text is sent.
    pub native_pdfs: bool,
    /// Let the provider clear old tool results itself (Anthropic context editing). Ignored
    /// by OpenAI-compatible servers.
    pub clear_tool_results: Option<ToolResultClearing>,
}

/// Server-side clearing of old tool results (Anthropic `clear_tool_uses_20250919`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolResultClearing {
    /// Clear once the prompt is above this many input tokens.
    pub trigger_tokens: u32,
    /// Most recent tool uses kept.
    pub keep: u32,
}

impl ChatRequest {
    pub fn new(model: impl Into<String>, messages: Vec<ChatMessage>) -> Self {
        Self {
            model: model.into(),
            system: None,
            system_context: None,
            messages,
            tools: Vec::new(),
            server_tools: Vec::new(),
            max_tokens: None,
            thinking: Thinking::Default,
            cache: false,
            effort: None,
            images: true,
            native_pdfs: true,
            clear_tool_results: None,
        }
    }
}

/// One step of a streamed reply.
#[derive(Debug, Clone, PartialEq)]
pub enum ChatEvent {
    TextDelta(String),
    ThinkingDelta(String),
    /// Opaque token that lets a thinking block be sent back to the provider later.
    ThinkingSignature(String),
    ToolUseStart {
        id: String,
        name: String,
    },
    /// A fragment of the tool call's JSON arguments.
    ToolUseDelta {
        id: String,
        partial_json: String,
    },
    ToolUseEnd {
        id: String,
    },
    /// The provider started running one of its own tools (Anthropic `server_tool_use`).
    /// Its input streams as [`ChatEvent::ToolUseDelta`] and ends with [`ChatEvent::ToolUseEnd`].
    ServerToolStart {
        id: String,
        name: String,
    },
    /// The result of a provider-run tool, exactly as sent (it must be sent back unchanged).
    ServerToolResult {
        tool_use_id: String,
        raw_json: String,
    },
    /// Token counts so far (cumulative; later events replace earlier ones).
    Usage(Usage),
    /// The reply is complete. Always the last event of a successful stream.
    Stop(StopReason),
}

/// A streamed reply. An `Err` item ends the stream.
pub type ChatStream = BoxStream<'static, Result<ChatEvent, LlmError>>;
