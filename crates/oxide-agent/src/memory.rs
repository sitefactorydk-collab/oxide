//! Learning memory items from a chat (opt-in): after a reply, a small model reads the latest
//! exchange and proposes facts worth keeping. They're saved, and the user can view, edit
//! and delete them in Settings → Memory.

use oxide_core::MemoryItem;
use oxide_core::memory::clean_memory_text;
use serde_json::Value;

/// Most items one exchange can add.
pub const MAX_NEW_ITEMS: usize = 3;

/// Where a proposed item applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Global,
    Project,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proposed {
    pub text: String,
    pub scope: Scope,
}

pub const MEMORY_SYSTEM: &str = "You maintain a short memory about the user of a chat app. \
     You answer with JSON only.";

/// The prompt for the memory model.
pub fn memory_prompt(
    existing: &[MemoryItem],
    project: Option<&str>,
    user_text: &str,
    reply_text: &str,
) -> String {
    let user: String = user_text.chars().take(4_000).collect();
    let reply: String = reply_text.chars().take(2_000).collect();
    let known = if existing.is_empty() {
        "(nothing yet)".to_owned()
    } else {
        existing
            .iter()
            .map(|m| format!("- {}", m.text))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let scope_rule = match project {
        Some(name) => format!(
            "The chat is in the project “{name}”. Use \"scope\": \"project\" for facts that \
             only matter for this project, and \"scope\": \"global\" for facts about the user \
             in general."
        ),
        None => "Use \"scope\": \"global\".".to_owned(),
    };
    format!(
        "Read the latest exchange and decide whether it tells you something about the user that \
         will still be useful in future chats: their name, role, preferences (language, tone, \
         format), ongoing projects, tools and setup, or something they explicitly asked to be \
         remembered. Don't save passing details, the task itself, things the assistant said, \
         secrets (passwords, keys, account numbers), health, or anything already known.\n\n\
         Known already:\n{known}\n\n{scope_rule}\n\n\
         Answer with JSON: {{\"memories\": [{{\"text\": \"…\", \"scope\": \"global\"}}]}}. \
         Each text is one short sentence in the third person (\"Prefers…\", \"Works on…\"). \
         At most {MAX_NEW_ITEMS} items. Most exchanges teach nothing new: then answer \
         {{\"memories\": []}}.\n\n\
         <user>\n{user}\n</user>\n<assistant>\n{reply}\n</assistant>"
    )
}

/// Parses the model's answer. Tolerates code fences and text around the JSON, and drops
/// duplicates of `existing` (case-insensitive) and of each other.
pub fn parse_proposals(raw: &str, existing: &[MemoryItem], in_project: bool) -> Vec<Proposed> {
    let (Some(start), Some(end)) = (raw.find('{'), raw.rfind('}')) else {
        return Vec::new();
    };
    if end < start {
        return Vec::new();
    }
    let Ok(v) = serde_json::from_str::<Value>(&raw[start..=end]) else {
        return Vec::new();
    };
    let Some(items) = v.get("memories").and_then(Value::as_array) else {
        return Vec::new();
    };
    let norm = |s: &str| {
        s.to_lowercase()
            .trim_end_matches('.')
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };
    let mut seen: Vec<String> = existing.iter().map(|m| norm(&m.text)).collect();
    let mut out = Vec::new();
    for item in items {
        let (text, scope) = match item {
            Value::String(s) => (s.as_str(), None),
            Value::Object(o) => (
                o.get("text").and_then(Value::as_str).unwrap_or(""),
                o.get("scope").and_then(Value::as_str),
            ),
            _ => continue,
        };
        let Some(text) = clean_memory_text(text) else {
            continue;
        };
        let key = norm(&text);
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        let scope = if in_project && scope == Some("project") {
            Scope::Project
        } else {
            Scope::Global
        };
        out.push(Proposed { text, scope });
        if out.len() == MAX_NEW_ITEMS {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(text: &str) -> MemoryItem {
        MemoryItem {
            id: "m".into(),
            project_id: None,
            text: text.into(),
            conversation_id: None,
            created_at: 0,
            updated_at: 0,
        }
    }

    #[test]
    fn prompt_lists_what_is_known() {
        let p = memory_prompt(&[item("Prefers Danish")], Some("Oxide"), "hi", "hello");
        assert!(p.contains("- Prefers Danish"));
        assert!(p.contains("project “Oxide”"));
        assert!(memory_prompt(&[], None, "a", "b").contains("(nothing yet)"));
    }

    #[test]
    fn proposals_are_parsed_leniently_and_deduplicated() {
        let raw = "Sure!\n```json\n{\"memories\": [\
            {\"text\": \"Prefers Danish.\", \"scope\": \"global\"},\
            {\"text\": \"Builds Oxide in Rust\", \"scope\": \"project\"},\
            \"Uses Windows 11\",\
            {\"text\": \"uses windows 11\"},\
            {\"text\": \"  \"},\
            {\"text\": \"Fourth\"}]}\n```";
        let got = parse_proposals(raw, &[item("prefers danish")], true);
        assert_eq!(
            got,
            vec![
                Proposed {
                    text: "Builds Oxide in Rust".into(),
                    scope: Scope::Project
                },
                Proposed {
                    text: "Uses Windows 11".into(),
                    scope: Scope::Global
                },
                Proposed {
                    text: "Fourth".into(),
                    scope: Scope::Global
                },
            ]
        );
        // Outside a project everything is global.
        let got = parse_proposals(
            r#"{"memories":[{"text":"x","scope":"project"}]}"#,
            &[],
            false,
        );
        assert_eq!(got[0].scope, Scope::Global);
        assert!(parse_proposals("{\"memories\": []}", &[], false).is_empty());
        assert!(parse_proposals("no json", &[], false).is_empty());
        assert!(parse_proposals("} {", &[], false).is_empty());
    }
}
