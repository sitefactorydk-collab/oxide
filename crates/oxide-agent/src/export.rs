//! Exporting a conversation as Markdown (the visible thread) or JSON (every branch).

use oxide_core::{ContentBlock, Conversation, Message, Role};
use serde::Serialize;

use crate::context::civil_date;

/// Identifies Oxide's JSON export format.
pub const EXPORT_FORMAT: &str = "oxide.conversation";
pub const EXPORT_VERSION: u32 = 1;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConversationExport<'a> {
    format: &'static str,
    version: u32,
    exported_at: i64,
    conversation: &'a Conversation,
    /// Every message, all branches, parents before children.
    messages: &'a [Message],
}

/// The whole conversation as pretty JSON, including every branch.
pub fn export_json(
    conversation: &Conversation,
    messages: &[Message],
    now_ms: i64,
) -> serde_json::Result<String> {
    serde_json::to_string_pretty(&ConversationExport {
        format: EXPORT_FORMAT,
        version: EXPORT_VERSION,
        exported_at: now_ms,
        conversation,
        messages,
    })
}

/// One thread (root to leaf) as Markdown. Thinking is left out; attachments and tool calls
/// are listed by name.
pub fn export_markdown(conversation: &Conversation, thread: &[Message], now_ms: i64) -> String {
    let (y, m, d) = civil_date(now_ms);
    let mut out = format!(
        "# {}\n\n_Exported from Oxide on {y:04}-{m:02}-{d:02} · Model: {}_\n",
        conversation.title.trim(),
        conversation.model
    );
    for msg in thread {
        let who = match msg.role {
            Role::User => "You".to_owned(),
            Role::Assistant => match msg.model.as_deref() {
                Some(model) if model != conversation.model => format!("Oxide ({model})"),
                _ => "Oxide".to_owned(),
            },
        };
        out.push_str(&format!("\n## {who}\n\n"));
        let mut parts: Vec<String> = Vec::new();
        let mut files: Vec<String> = Vec::new();
        for block in &msg.content {
            match block {
                ContentBlock::Text { text } if !text.trim().is_empty() => {
                    parts.push(text.trim_end().to_owned());
                }
                ContentBlock::Image { .. } => files.push("an image".to_owned()),
                ContentBlock::Document { name, .. } => files.push(format!("`{name}`")),
                ContentBlock::ToolUse { name, .. } => {
                    parts.push(format!("> Tool call: `{name}`"));
                }
                ContentBlock::ArtifactRef { title, version, .. } => {
                    parts.push(format!("> Artifact: {title} (version {version})"));
                }
                _ => {}
            }
        }
        if !files.is_empty() {
            parts.insert(0, format!("> Attached: {}", files.join(", ")));
        }
        if parts.is_empty() {
            parts.push("_(no text)_".to_owned());
        }
        out.push_str(&parts.join("\n\n"));
        out.push('\n');
    }
    out
}

/// A file name for an export: the title without characters Windows forbids.
pub fn export_file_name(title: &str, ext: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                ' '
            } else {
                c
            }
        })
        .collect();
    let words: Vec<&str> = cleaned.split_whitespace().collect();
    let mut name: String = words.join(" ").chars().take(80).collect();
    name = name.trim_end_matches(['.', ' ']).to_owned();
    if name.is_empty() {
        name = "Conversation".into();
    }
    format!("{name}.{ext}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxide_core::MediaSource;

    fn msg(id: &str, role: Role, content: Vec<ContentBlock>, model: Option<&str>) -> Message {
        Message {
            id: id.into(),
            conversation_id: "c".into(),
            parent_id: None,
            role,
            content,
            model: model.map(Into::into),
            created_at: 0,
            stop_reason: None,
            usage: None,
            feedback: None,
        }
    }

    fn convo() -> Conversation {
        Conversation {
            id: "c".into(),
            title: "Rust: lifetimes?".into(),
            project_id: None,
            model: "anthropic/claude-x".into(),
            starred: false,
            created_at: 0,
            updated_at: 0,
        }
    }

    #[test]
    fn markdown_lists_turns_files_and_skips_thinking() {
        let thread = vec![
            msg(
                "1",
                Role::User,
                vec![
                    ContentBlock::Document {
                        media_type: "text/plain".into(),
                        name: "main.rs".into(),
                        source: MediaSource::Path { path: "x".into() },
                    },
                    ContentBlock::text("Why does this fail?"),
                ],
                None,
            ),
            msg(
                "2",
                Role::Assistant,
                vec![
                    ContentBlock::Thinking {
                        text: "secret".into(),
                        signature: None,
                        duration_ms: None,
                    },
                    ContentBlock::text("Because of `'a`.\n\n```rust\nfn f() {}\n```\n"),
                ],
                Some("anthropic/claude-y"),
            ),
        ];
        let md = export_markdown(&convo(), &thread, 1_793_880_000_000);
        assert!(md.starts_with("# Rust: lifetimes?\n\n_Exported from Oxide on 2026-11-05"));
        assert!(md.contains("## You\n\n> Attached: `main.rs`\n\nWhy does this fail?"));
        assert!(md.contains("## Oxide (anthropic/claude-y)\n\nBecause of"));
        assert!(md.contains("```rust\nfn f() {}\n```\n"));
        assert!(!md.contains("secret"));
    }

    #[test]
    fn json_has_a_format_marker_and_all_messages() {
        let messages = vec![msg("1", Role::User, vec![ContentBlock::text("hi")], None)];
        let json = export_json(&convo(), &messages, 7).expect("serializes");
        let v: serde_json::Value = serde_json::from_str(&json).expect("parses");
        assert_eq!(v["format"], EXPORT_FORMAT);
        assert_eq!(v["exportedAt"], 7);
        assert_eq!(v["conversation"]["title"], "Rust: lifetimes?");
        assert_eq!(v["messages"][0]["content"][0]["text"], "hi");
    }

    #[test]
    fn file_names_are_safe() {
        assert_eq!(
            export_file_name("Rust: lifetimes?", "md"),
            "Rust lifetimes.md"
        );
        assert_eq!(export_file_name("  ///  ", "json"), "Conversation.json");
        assert_eq!(export_file_name("a.", "md"), "a.md");
    }
}
