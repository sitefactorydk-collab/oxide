//! Generating replies: options, and the events that stream a reply to the UI.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{
    Compaction, ContentBlock, ContextUsage, Conversation, Effort, Message, Timestamp,
    ToolApprovalRequest,
};

/// Per-request options chosen in the composer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GenerateOptions {
    /// Extended thinking, for models that support it. Ignored by the others.
    pub thinking: bool,
    /// Effort level. `None` (and levels the model doesn't accept) leave the model's default.
    pub effort: Option<Effort>,
}

/// Why a reply ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum StopReason {
    /// The model finished its answer.
    EndTurn,
    /// The output token limit was reached.
    MaxTokens,
    StopSequence,
    /// The model wants to call a tool. The tool loop runs it and continues; a saved reply only
    /// ends this way if its tools couldn't be run.
    ToolUse,
    /// The provider paused a long turn (Anthropic server tools); the tool loop continues it.
    PauseTurn,
    /// The reply used the most tool steps allowed (Settings → Permissions).
    StepLimit,
    /// The model declined to answer.
    Refusal,
    /// The context window is full.
    ContextFull,
    /// The user pressed Stop.
    Stopped,
    /// The request failed part-way; the content is what arrived before the error.
    Error,
    /// A reason Oxide doesn't know.
    Other,
}

impl StopReason {
    /// The value stored in the database (same as the serde name).
    pub fn as_str(self) -> &'static str {
        match self {
            StopReason::EndTurn => "endTurn",
            StopReason::MaxTokens => "maxTokens",
            StopReason::StopSequence => "stopSequence",
            StopReason::ToolUse => "toolUse",
            StopReason::PauseTurn => "pauseTurn",
            StopReason::StepLimit => "stepLimit",
            StopReason::Refusal => "refusal",
            StopReason::ContextFull => "contextFull",
            StopReason::Stopped => "stopped",
            StopReason::Error => "error",
            StopReason::Other => "other",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "endTurn" => StopReason::EndTurn,
            "maxTokens" => StopReason::MaxTokens,
            "stopSequence" => StopReason::StopSequence,
            "toolUse" => StopReason::ToolUse,
            "pauseTurn" => StopReason::PauseTurn,
            "stepLimit" => StopReason::StepLimit,
            "refusal" => StopReason::Refusal,
            "contextFull" => StopReason::ContextFull,
            "stopped" => StopReason::Stopped,
            "error" => StopReason::Error,
            _ => StopReason::Other,
        }
    }
}

/// Token counts for one reply.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    /// Prompt tokens served from the provider's cache.
    pub cache_read_tokens: u32,
    /// Prompt tokens written to the provider's cache.
    pub cache_write_tokens: u32,
}

impl Usage {
    /// Adds counts reported in several parts (streams report input first, output last).
    /// Non-zero values from `other` replace this one's.
    pub fn merge(&mut self, other: Usage) {
        let pick = |a: &mut u32, b: u32| {
            if b > 0 {
                *a = b;
            }
        };
        pick(&mut self.input_tokens, other.input_tokens);
        pick(&mut self.output_tokens, other.output_tokens);
        pick(&mut self.cache_read_tokens, other.cache_read_tokens);
        pick(&mut self.cache_write_tokens, other.cache_write_tokens);
    }
}

/// One step of a reply being written. Sent to the UI as it happens.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum StreamEvent {
    /// A reply to `parent_id` has started. `message_id` is the id it will be saved under.
    #[serde(rename_all = "camelCase")]
    Started {
        message_id: String,
        parent_id: String,
        model: String,
        started_at: Timestamp,
    },
    TextDelta {
        text: String,
    },
    ThinkingDelta {
        text: String,
    },
    /// The model started a tool call; its arguments are still streaming.
    ToolUse {
        id: String,
        name: String,
    },
    /// More of a tool call's arguments (JSON text, in pieces). Lets the UI show an artifact
    /// while it's being written.
    #[serde(rename_all = "camelCase")]
    ToolInputDelta {
        id: String,
        partial_json: String,
    },
    /// A tool call's arguments are complete. `server` is the MCP server (`None` for built-in
    /// tools, `Some("anthropic")` for a tool the provider runs itself); `tool` is the tool's
    /// own name and `title` its display name.
    #[serde(rename_all = "camelCase")]
    ToolCall {
        id: String,
        name: String,
        server: Option<String>,
        tool: String,
        title: Option<String>,
        input_json: String,
    },
    /// A tool call waits for the user's approval (answer with `answer_tool_approval`).
    ToolApproval {
        request: ToolApprovalRequest,
    },
    /// A tool call was approved (or needed no approval) and is running.
    ToolRunning {
        id: String,
    },
    /// How far a long tool call has come (a video being made).
    ToolProgress {
        id: String,
        progress: crate::generation::ToolProgress,
    },
    /// A tool call finished. `content` is what the model sees.
    #[serde(rename_all = "camelCase")]
    ToolResult {
        id: String,
        content: Vec<ContentBlock>,
        is_error: bool,
        duration_ms: u32,
    },
    Usage {
        usage: Usage,
    },
    /// How full the context is, just before a request goes to the model.
    Context {
        usage: ContextUsage,
    },
    /// Oxide is summarizing the start of the conversation to make room.
    Compacting,
    /// A summary now stands in for the start of the conversation (also saved with the chat).
    Compacted {
        compaction: Compaction,
    },
    /// The reply is saved. `message` is `None` when it was stopped before any output.
    Done {
        message: Option<Message>,
    },
    /// The reply failed. `message` is the partial reply if any of it was saved.
    Failed {
        error: String,
        message: Option<Message>,
    },
}

/// A [`StreamEvent`] for one conversation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ChatStreamEvent {
    pub conversation_id: String,
    pub event: StreamEvent,
}

/// A chat changed in the background (for example, it got an automatic title).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ConversationChanged {
    pub conversation: Conversation,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stop_reasons_round_trip() {
        for r in [
            StopReason::EndTurn,
            StopReason::MaxTokens,
            StopReason::StopSequence,
            StopReason::ToolUse,
            StopReason::PauseTurn,
            StopReason::StepLimit,
            StopReason::Refusal,
            StopReason::ContextFull,
            StopReason::Stopped,
            StopReason::Error,
            StopReason::Other,
        ] {
            assert_eq!(StopReason::parse(r.as_str()), r);
            let json = serde_json::to_string(&r).expect("serializes");
            assert_eq!(json, format!("\"{}\"", r.as_str()));
        }
    }

    #[test]
    fn usage_merge_keeps_reported_parts() {
        let mut u = Usage {
            input_tokens: 10,
            cache_read_tokens: 4,
            ..Usage::default()
        };
        u.merge(Usage {
            output_tokens: 7,
            ..Usage::default()
        });
        assert_eq!(
            u,
            Usage {
                input_tokens: 10,
                output_tokens: 7,
                cache_read_tokens: 4,
                cache_write_tokens: 0
            }
        );
    }

    #[test]
    fn stream_events_are_tagged() {
        let json =
            serde_json::to_value(StreamEvent::TextDelta { text: "hi".into() }).expect("serializes");
        assert_eq!(json, serde_json::json!({"type": "textDelta", "text": "hi"}));
        let json = serde_json::to_value(StreamEvent::Started {
            message_id: "m".into(),
            parent_id: "p".into(),
            model: "a/b".into(),
            started_at: 1,
        })
        .expect("serializes");
        assert_eq!(json["messageId"], "m");
    }
}
