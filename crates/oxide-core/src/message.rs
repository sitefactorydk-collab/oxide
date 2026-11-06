//! Messages and their content blocks.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::Timestamp;
use crate::chat::{StopReason, Usage};

/// Who wrote a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
}

impl Role {
    /// The value stored in the database.
    pub fn as_str(self) -> &'static str {
        match self {
            Role::User => "user",
            Role::Assistant => "assistant",
        }
    }

    /// Parses the stored value.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "user" => Some(Role::User),
            "assistant" => Some(Role::Assistant),
            _ => None,
        }
    }
}

/// One message in a conversation.
///
/// `parent_id` makes the messages a tree: editing a message adds a sibling under the same
/// parent, and regenerating a reply adds a sibling reply. The visible thread is one path
/// from the root to a leaf.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: String,
    pub conversation_id: String,
    pub parent_id: Option<String>,
    pub role: Role,
    pub content: Vec<ContentBlock>,
    /// Model that wrote an assistant message.
    pub model: Option<String>,
    pub created_at: Timestamp,
    /// Why an assistant reply ended. `None` for user messages.
    pub stop_reason: Option<StopReason>,
    /// Token counts of an assistant reply, when the provider reported them.
    pub usage: Option<Usage>,
    /// The user's rating of an assistant reply. Stored locally; never sent anywhere.
    pub feedback: Option<Feedback>,
}

/// Thumbs up or down on a reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Rating {
    Good,
    Bad,
}

/// A rating with an optional note ("what went wrong").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Feedback {
    pub rating: Rating,
    pub note: Option<String>,
    pub created_at: Timestamp,
}

/// Where the bytes of an image or document live.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum MediaSource {
    /// Inline base64 data.
    Base64 { data: String },
    /// A file on disk: an attachment's copy in the app's data folder.
    Path { path: String },
}

/// A piece of message content. Mirrors the block types of the model APIs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    #[serde(rename_all = "camelCase")]
    Image {
        media_type: String,
        source: MediaSource,
    },
    #[serde(rename_all = "camelCase")]
    Document {
        media_type: String,
        name: String,
        source: MediaSource,
    },
    /// Extended-thinking output. `signature` is the provider's opaque token for replaying it.
    #[serde(rename_all = "camelCase")]
    Thinking {
        text: String,
        signature: Option<String>,
        duration_ms: Option<u32>,
    },
    /// A tool call made by the model. `input_json` is the JSON arguments as text; `name` is
    /// the name the model used (MCP tools are `server__tool`).
    ///
    /// `server` (the MCP server; `None` for Oxide's built-in tools) and `tool` (the tool's own
    /// name) are filled in by the tool loop, so the UI can label the call.
    #[serde(rename_all = "camelCase")]
    ToolUse {
        id: String,
        name: String,
        input_json: String,
        #[serde(default)]
        server: Option<String>,
        #[serde(default)]
        tool: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    ToolResult {
        tool_use_id: String,
        content: Vec<ContentBlock>,
        is_error: bool,
    },
    /// A tool the provider ran itself during the reply (Anthropic's web search).
    #[serde(rename_all = "camelCase")]
    ServerToolUse {
        id: String,
        name: String,
        input_json: String,
    },
    /// The result of a [`ContentBlock::ServerToolUse`], kept exactly as the provider sent it
    /// (`raw_json`), because it must be sent back unchanged while the reply continues.
    #[serde(rename_all = "camelCase")]
    ServerToolResult {
        tool_use_id: String,
        raw_json: String,
    },
    #[serde(rename_all = "camelCase")]
    ArtifactRef {
        artifact_id: String,
        title: String,
        version: u32,
    },
}

impl ContentBlock {
    /// A plain text block.
    pub fn text(text: impl Into<String>) -> Self {
        ContentBlock::Text { text: text.into() }
    }
}

/// The searchable text of some content: text blocks and tool-result text, joined by blank lines.
/// Thinking, tool inputs and media are left out.
pub fn plain_text(content: &[ContentBlock]) -> String {
    let mut parts: Vec<&str> = Vec::new();
    collect_text(content, &mut parts);
    parts.join("\n\n")
}

fn collect_text<'a>(content: &'a [ContentBlock], out: &mut Vec<&'a str>) {
    for block in content {
        match block {
            ContentBlock::Text { text } if !text.trim().is_empty() => out.push(text),
            ContentBlock::ToolResult { content, .. } => collect_text(content, out),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_blocks_use_camel_case_tags() {
        let block = ContentBlock::ToolUse {
            id: "t1".into(),
            name: "search".into(),
            input_json: "{}".into(),
            server: None,
            tool: None,
        };
        let json = serde_json::to_value(&block).expect("serializes");
        assert_eq!(json["type"], "toolUse");
        assert_eq!(json["inputJson"], "{}");
        // Blocks saved before Phase 6 have no server/tool fields.
        let old: ContentBlock =
            serde_json::from_str(r#"{"type":"toolUse","id":"t","name":"n","inputJson":"{}"}"#)
                .expect("parses");
        assert!(matches!(
            old,
            ContentBlock::ToolUse {
                server: None,
                tool: None,
                ..
            }
        ));
    }

    #[test]
    fn content_round_trips() {
        let content = vec![
            ContentBlock::text("hi"),
            ContentBlock::Thinking {
                text: "hmm".into(),
                signature: None,
                duration_ms: Some(1200),
            },
            ContentBlock::Image {
                media_type: "image/png".into(),
                source: MediaSource::Path {
                    path: "a.png".into(),
                },
            },
        ];
        let json = serde_json::to_string(&content).expect("serializes");
        let back: Vec<ContentBlock> = serde_json::from_str(&json).expect("deserializes");
        assert_eq!(back, content);
    }

    #[test]
    fn plain_text_skips_thinking_and_tools() {
        let content = vec![
            ContentBlock::Thinking {
                text: "secret plan".into(),
                signature: None,
                duration_ms: None,
            },
            ContentBlock::text("Visible answer"),
            ContentBlock::ToolResult {
                tool_use_id: "t".into(),
                content: vec![ContentBlock::text("tool output")],
                is_error: false,
            },
        ];
        assert_eq!(plain_text(&content), "Visible answer\n\ntool output");
    }

    #[test]
    fn role_round_trips() {
        for r in [Role::User, Role::Assistant] {
            assert_eq!(Role::parse(r.as_str()), Some(r));
        }
        assert_eq!(Role::parse("system"), None);
    }
}
