//! Memory: short facts about the user that Oxide keeps across chats (opt-in).

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::Timestamp;

/// Longest memory item, in characters.
pub const MAX_MEMORY_CHARS: usize = 500;

/// One remembered item. `project_id = None` means it applies to every chat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MemoryItem {
    pub id: String,
    pub project_id: Option<String>,
    pub text: String,
    /// The chat it was learned from (`None` when the user wrote it).
    pub conversation_id: Option<String>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct NewMemory {
    pub project_id: Option<String>,
    pub text: String,
}

/// Memory items as the bullet list the system prompt carries.
pub fn memory_prompt(items: &[MemoryItem]) -> Option<String> {
    let lines: Vec<String> = items
        .iter()
        .map(|m| m.text.trim())
        .filter(|t| !t.is_empty())
        .map(|t| format!("- {t}"))
        .collect();
    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}

/// Text as it's stored: one line, trimmed, at most [`MAX_MEMORY_CHARS`]. `None` if empty.
pub fn clean_memory_text(text: &str) -> Option<String> {
    let one_line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let t = one_line.trim_start_matches(['-', '*', '•', ' ']).trim();
    if t.is_empty() {
        None
    } else {
        Some(t.chars().take(MAX_MEMORY_CHARS).collect())
    }
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
    fn prompt_and_cleaning() {
        assert_eq!(memory_prompt(&[]), None);
        assert_eq!(
            memory_prompt(&[item("Prefers Danish"), item(" "), item("Uses Windows")]).as_deref(),
            Some("- Prefers Danish\n- Uses Windows")
        );
        assert_eq!(
            clean_memory_text("  - Likes\n  Rust "),
            Some("Likes Rust".into())
        );
        assert_eq!(clean_memory_text(" • "), None);
        assert_eq!(
            clean_memory_text(&"x".repeat(900)).map(|t| t.len()),
            Some(MAX_MEMORY_CHARS)
        );
    }
}
