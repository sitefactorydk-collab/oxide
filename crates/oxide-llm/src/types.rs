//! Provider-neutral request and event types.

use futures_util::stream::BoxStream;
use oxide_core::{ContentBlock, Role, StopReason, Usage};
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
    pub messages: Vec<ChatMessage>,
    pub tools: Vec<ToolSpec>,
    /// Output token limit. Required by Anthropic (a default is used when `None`); omitted for
    /// OpenAI-compatible servers when `None`, so their own default applies.
    pub max_tokens: Option<u32>,
    pub thinking: Thinking,
    /// Mark prompt-cache breakpoints (Anthropic): system prompt, tools and the latest turns.
    pub cache: bool,
}

impl ChatRequest {
    pub fn new(model: impl Into<String>, messages: Vec<ChatMessage>) -> Self {
        Self {
            model: model.into(),
            system: None,
            messages,
            tools: Vec::new(),
            max_tokens: None,
            thinking: Thinking::Default,
            cache: false,
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
    /// Token counts so far (cumulative; later events replace earlier ones).
    Usage(Usage),
    /// The reply is complete. Always the last event of a successful stream.
    Stop(StopReason),
}

/// A streamed reply. An `Err` item ends the stream.
pub type ChatStream = BoxStream<'static, Result<ChatEvent, LlmError>>;
