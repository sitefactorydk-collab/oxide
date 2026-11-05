//! Conversations (chats) and search results over them.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::Timestamp;

/// Longest title derived from a first message, in characters (an ellipsis is added).
pub const MAX_DERIVED_TITLE_CHARS: usize = 48;

/// Title used when a chat has no text to derive one from.
pub const DEFAULT_TITLE: &str = "New chat";

/// One chat. Messages hang off it as a tree (see [`crate::Message::parent_id`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub project_id: Option<String>,
    /// Model id chosen for this chat (`<provider>/<model>`).
    pub model: String,
    pub starred: bool,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// Creates a chat together with its first user message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct StartConversation {
    pub text: String,
    pub model: String,
    pub project_id: Option<String>,
}

/// A chat matching a search, with the best matching message excerpt if the hit was in the text.
///
/// `snippet` marks matched terms with U+0002 (start) and U+0003 (end).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub conversation: Conversation,
    pub message_id: Option<String>,
    pub snippet: Option<String>,
}

/// Derives a chat title from the first line of a message.
pub fn title_from_text(text: &str) -> String {
    let first = text.trim().lines().next().unwrap_or("").trim();
    if first.is_empty() {
        return DEFAULT_TITLE.to_owned();
    }
    if first.chars().count() > MAX_DERIVED_TITLE_CHARS {
        let cut: String = first.chars().take(MAX_DERIVED_TITLE_CHARS - 1).collect();
        format!("{}…", cut.trim_end())
    } else {
        first.to_owned()
    }
}

/// Normalizes a user-entered title: trims it and caps its length. `None` if it's blank.
pub fn clean_title(title: &str) -> Option<String> {
    let t = title.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.chars().take(200).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_uses_first_line() {
        assert_eq!(title_from_text("  Hello there\nsecond line"), "Hello there");
    }

    #[test]
    fn title_is_capped_with_ellipsis() {
        let t = title_from_text(&"æ".repeat(100));
        assert_eq!(t.chars().count(), MAX_DERIVED_TITLE_CHARS);
        assert!(t.ends_with('…'));
    }

    #[test]
    fn blank_text_gets_default_title() {
        assert_eq!(title_from_text(" \n "), DEFAULT_TITLE);
    }

    #[test]
    fn clean_title_rejects_blank() {
        assert_eq!(clean_title("   "), None);
        assert_eq!(clean_title("  Plan  ").as_deref(), Some("Plan"));
    }
}
