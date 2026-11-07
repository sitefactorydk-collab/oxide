//! Projects group chats under shared instructions and knowledge files.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::Timestamp;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub description: String,
    pub instructions: String,
    /// Archived projects are hidden from the main list; their chats keep working.
    pub archived: bool,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct NewProject {
    pub name: String,
    pub description: String,
}

/// Partial update: `None` leaves a field as it is.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProjectPatch {
    pub name: Option<String>,
    pub description: Option<String>,
    pub instructions: Option<String>,
    pub archived: Option<bool>,
}

/// Longest project instructions, in characters.
pub const MAX_INSTRUCTIONS_CHARS: usize = 20_000;

/// Where a knowledge file's text came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum KnowledgeKind {
    /// A text or code file.
    Text,
    /// A PDF; its extracted text is stored.
    Pdf,
    /// Text typed or pasted into the project ("Add text content").
    Note,
}

impl KnowledgeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            KnowledgeKind::Text => "text",
            KnowledgeKind::Pdf => "pdf",
            KnowledgeKind::Note => "note",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "pdf" => KnowledgeKind::Pdf,
            "note" => KnowledgeKind::Note,
            _ => KnowledgeKind::Text,
        }
    }
}

/// One file in a project's knowledge. Its text lives in the database.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeFile {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub kind: KnowledgeKind,
    pub media_type: String,
    /// Characters of stored text.
    pub chars: u32,
    /// Estimated tokens of the text (see [`estimate_tokens`]).
    pub tokens: u32,
    pub pages: Option<u32>,
    /// Something the user should know (lossy text, few words found).
    pub note: Option<String>,
    pub created_at: Timestamp,
}

/// A knowledge file with its text (the file viewer).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeText {
    pub file: KnowledgeFile,
    pub text: String,
}

/// How a project's knowledge reaches the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum KnowledgeMode {
    /// No files.
    Empty,
    /// Every file goes into the prompt in full.
    Full,
    /// Too large for the budget: the model searches it with `project_search`.
    Search,
}

/// A project's knowledge files and how they're used.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProjectKnowledge {
    pub files: Vec<KnowledgeFile>,
    pub total_tokens: u32,
    /// The knowledge budget from Settings (the model's context can lower it in a chat).
    pub budget_tokens: u32,
    pub mode: KnowledgeMode,
}

/// A passage found by knowledge search.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeHit {
    pub file_id: String,
    pub file_name: String,
    /// Position of the passage in its file (0-based).
    pub chunk: u32,
    /// Character offset of the passage in the file's text.
    pub start: u32,
    pub text: String,
    /// BM25 score (higher is better).
    pub score: f64,
}

/// Rough token count of a text: about four characters per token.
pub fn estimate_tokens(chars: usize) -> u32 {
    u32::try_from(chars.div_ceil(4)).unwrap_or(u32::MAX)
}

/// The mode for a total size and budget.
pub fn knowledge_mode(total_tokens: u32, files: usize, budget: u32) -> KnowledgeMode {
    if files == 0 {
        KnowledgeMode::Empty
    } else if total_tokens <= budget {
        KnowledgeMode::Full
    } else {
        KnowledgeMode::Search
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_and_modes() {
        assert_eq!(estimate_tokens(0), 0);
        assert_eq!(estimate_tokens(9), 3);
        assert_eq!(knowledge_mode(0, 0, 100), KnowledgeMode::Empty);
        assert_eq!(knowledge_mode(100, 1, 100), KnowledgeMode::Full);
        assert_eq!(knowledge_mode(101, 2, 100), KnowledgeMode::Search);
    }

    #[test]
    fn patches_without_archived_parse_and_kinds_round_trip() {
        let p: ProjectPatch = serde_json::from_str(r#"{"name":"n"}"#).expect("parses");
        assert_eq!(p.archived, None);
        assert_eq!(KnowledgeKind::parse("pdf"), KnowledgeKind::Pdf);
        assert_eq!(KnowledgeKind::parse("x"), KnowledgeKind::Text);
    }
}
