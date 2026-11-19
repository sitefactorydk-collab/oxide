//! Exporting a conversation as Markdown or a self-contained HTML page (the visible thread),
//! or JSON (every branch).

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

/// Largest image embedded in an HTML export; bigger ones are listed by name only.
pub const MAX_HTML_IMAGE_BYTES: usize = 4 * 1024 * 1024;

const HTML_STYLE: &str = "
:root{color-scheme:light dark;--bg:#faf9f5;--fg:#1f1e1b;--muted:#6b6a65;--line:#e5e2d9;--bubble:#f0eee6;--code:#f3f1ea;--accent:#b5562f}
@media (prefers-color-scheme:dark){:root{--bg:#262624;--fg:#ecebe6;--muted:#a3a29c;--line:#3d3c38;--bubble:#353431;--code:#1f1e1c;--accent:#e07a4f}}
*{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--fg);font:16px/1.6 system-ui,-apple-system,'Segoe UI',sans-serif}
main{max-width:768px;margin:0 auto;padding:40px 20px 80px}
header h1{font:600 1.6rem/1.3 Georgia,'Source Serif 4',serif;margin:0 0 4px}header p{color:var(--muted);margin:0 0 32px;font-size:.875rem}
.turn{margin:28px 0}.who{font-size:.75rem;font-weight:600;color:var(--muted);text-transform:uppercase;letter-spacing:.04em;margin-bottom:6px}
.user{display:flex;flex-direction:column;align-items:flex-end}.user .body{background:var(--bubble);border-radius:16px;padding:10px 16px;max-width:85%}
.assistant .body{font-family:Georgia,'Source Serif 4',serif}
.body>:first-child{margin-top:0}.body>:last-child{margin-bottom:0}
pre{background:var(--code);border:1px solid var(--line);border-radius:8px;padding:12px;overflow:auto;font:13px/1.5 ui-monospace,'JetBrains Mono',Consolas,monospace}
code{font-family:ui-monospace,'JetBrains Mono',Consolas,monospace;font-size:.9em;background:var(--code);padding:1px 4px;border-radius:4px}pre code{background:none;padding:0}
blockquote{margin:0;padding-left:14px;border-left:3px solid var(--line);color:var(--muted)}
table{border-collapse:collapse;margin:12px 0;font-family:system-ui,sans-serif;font-size:.9rem}th,td{border:1px solid var(--line);padding:6px 10px;text-align:left}
a{color:var(--accent)}img{max-width:100%;border-radius:8px;margin:6px 0}
.note{font-family:system-ui,sans-serif;font-size:.8rem;color:var(--muted);border:1px solid var(--line);border-radius:8px;padding:4px 10px;display:inline-block;margin:4px 0}
footer{color:var(--muted);font-size:.75rem;margin-top:48px;border-top:1px solid var(--line);padding-top:12px}
";

/// One thread (root to leaf) as a self-contained HTML page: no scripts, nothing loaded from
/// the network (the page's own CSP says so), images embedded when `image_uri` returns a
/// `data:` URI for them. Thinking is left out; tool calls and artifacts are noted by name.
pub fn export_html(
    conversation: &Conversation,
    thread: &[Message],
    now_ms: i64,
    image_uri: &dyn Fn(&ContentBlock) -> Option<String>,
) -> String {
    use crate::markdown_html::{escape, render};
    let (y, m, d) = civil_date(now_ms);
    let title = escape(conversation.title.trim());
    let mut out = format!(
        "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; img-src data:; style-src 'unsafe-inline'\">\n\
         <meta name=\"generator\" content=\"Oxide {}\">\n<title>{title}</title>\n<style>{HTML_STYLE}</style>\n</head>\n<body>\n<main>\n\
         <header><h1>{title}</h1><p>Exported from Oxide on {y:04}-{m:02}-{d:02} · Model: {}</p></header>\n",
        escape(oxide_core::version()),
        escape(&conversation.model)
    );
    for msg in thread {
        let (class, who) = match msg.role {
            Role::User => ("user", "You".to_owned()),
            Role::Assistant => (
                "assistant",
                match msg.model.as_deref() {
                    Some(model) if model != conversation.model => format!("Oxide ({model})"),
                    _ => "Oxide".to_owned(),
                },
            ),
        };
        out.push_str(&format!(
            "<section class=\"turn {class}\">\n<div class=\"who\">{}</div>\n<div class=\"body\">\n",
            escape(&who)
        ));
        let mut wrote = false;
        for block in &msg.content {
            match block {
                ContentBlock::Text { text } if !text.trim().is_empty() => {
                    out.push_str(&render(text));
                    wrote = true;
                }
                ContentBlock::Image { .. } => {
                    match image_uri(block).filter(|u| u.starts_with("data:image/")) {
                        Some(uri) => out.push_str(&format!(
                            "<p><img alt=\"Attached image\" src=\"{}\"></p>\n",
                            escape(&uri)
                        )),
                        None => out.push_str("<p class=\"note\">Attached: an image</p>\n"),
                    }
                    wrote = true;
                }
                ContentBlock::Document { name, .. } => {
                    out.push_str(&format!(
                        "<p class=\"note\">Attached: {}</p>\n",
                        escape(name)
                    ));
                    wrote = true;
                }
                ContentBlock::ToolUse { name, .. } => {
                    out.push_str(&format!(
                        "<p class=\"note\">Tool call: <code>{}</code></p>\n",
                        escape(name)
                    ));
                    wrote = true;
                }
                ContentBlock::ArtifactRef { title, version, .. } => {
                    out.push_str(&format!(
                        "<p class=\"note\">Artifact: {} (version {version})</p>\n",
                        escape(title)
                    ));
                    wrote = true;
                }
                _ => {}
            }
        }
        if !wrote {
            out.push_str("<p class=\"note\">(no text)</p>\n");
        }
        out.push_str("</div>\n</section>\n");
    }
    out.push_str("<footer>Made with Oxide. Thinking and tool results are not included.</footer>\n</main>\n</body>\n</html>\n");
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
    fn html_is_self_contained_escaped_and_skips_thinking() {
        let mut c = convo();
        c.title = "<Plan> & \"go\"".into();
        let thread = vec![
            msg(
                "1",
                Role::User,
                vec![
                    ContentBlock::Image {
                        media_type: "image/png".into(),
                        source: MediaSource::Base64 {
                            data: "iVBO".into(),
                        },
                    },
                    ContentBlock::text("What is <b>this</b>?"),
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
                    ContentBlock::text("A **logo**.\n\n- one\n- two"),
                    ContentBlock::ToolUse {
                        id: "t".into(),
                        name: "web_search".into(),
                        input_json: "{}".into(),
                        server: None,
                        tool: None,
                    },
                ],
                None,
            ),
        ];
        let html = export_html(&c, &thread, 1_793_880_000_000, &|b| match b {
            ContentBlock::Image { .. } => Some("data:image/png;base64,iVBO".into()),
            _ => None,
        });
        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(html.contains("<title>&lt;Plan&gt; &amp; &quot;go&quot;</title>"));
        assert!(html.contains("default-src 'none'; img-src data:"));
        assert!(html.contains("<img alt=\"Attached image\" src=\"data:image/png;base64,iVBO\">"));
        assert!(html.contains("What is &lt;b&gt;this&lt;/b&gt;?"));
        assert!(html.contains("<strong>logo</strong>"));
        assert!(html.contains("<li>two</li>"));
        assert!(html.contains("Tool call: <code>web_search</code>"));
        assert!(!html.contains("secret"));
        assert!(!html.contains("<script"));
        // A non-image URI isn't embedded.
        let html = export_html(&c, &thread, 0, &|_| Some("javascript:alert(1)".into()));
        assert!(html.contains("Attached: an image"));
        assert!(!html.contains("javascript:"));
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
