//! Artifacts: documents, code, diagrams and small apps a model makes in a chat with the
//! built-in `artifact` tool. Each change is a new version; the side panel shows them.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::Timestamp;

/// The `artifact` tool's name.
pub const ARTIFACT_TOOL: &str = "artifact";
/// Most characters one artifact version holds.
pub const MAX_ARTIFACT_CHARS: usize = 1_000_000;
/// Longest artifact key (the id the model chooses).
pub const MAX_KEY_CHARS: usize = 64;
/// Longest title.
pub const MAX_TITLE_CHARS: usize = 120;

/// How an artifact is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ArtifactKind {
    /// A Markdown document.
    Markdown,
    /// Source code in `language`.
    Code,
    /// A web page, run in a sandboxed frame.
    Html,
    /// An SVG image.
    Svg,
    /// A Mermaid diagram.
    Mermaid,
    /// A React component (default export), run in a sandboxed frame.
    React,
}

impl ArtifactKind {
    pub const ALL: [ArtifactKind; 6] = [
        ArtifactKind::Markdown,
        ArtifactKind::Code,
        ArtifactKind::Html,
        ArtifactKind::Svg,
        ArtifactKind::Mermaid,
        ArtifactKind::React,
    ];

    /// The stored value (also the `type` the tool takes).
    pub fn as_str(self) -> &'static str {
        match self {
            ArtifactKind::Markdown => "markdown",
            ArtifactKind::Code => "code",
            ArtifactKind::Html => "html",
            ArtifactKind::Svg => "svg",
            ArtifactKind::Mermaid => "mermaid",
            ArtifactKind::React => "react",
        }
    }

    /// Parses a stored value or what a model wrote; forgiving about common spellings
    /// (`md`, `text/markdown`, `image/svg+xml`, `jsx`, …).
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim().to_ascii_lowercase();
        let s = s.rsplit('/').next().unwrap_or(&s).to_owned();
        Some(match s.as_str() {
            "markdown" | "md" | "document" | "text" | "x-markdown" => ArtifactKind::Markdown,
            "code" | "source" => ArtifactKind::Code,
            "html" | "htm" | "webpage" | "page" => ArtifactKind::Html,
            "svg" | "svg+xml" | "image" => ArtifactKind::Svg,
            "mermaid" | "diagram" | "x-mermaid" => ArtifactKind::Mermaid,
            "react" | "jsx" | "tsx" | "component" => ArtifactKind::React,
            _ => return None,
        })
    }

    /// File extension for downloads.
    pub fn extension(self, language: Option<&str>) -> &'static str {
        match self {
            ArtifactKind::Markdown => "md",
            ArtifactKind::Html => "html",
            ArtifactKind::Svg => "svg",
            ArtifactKind::Mermaid => "mmd",
            ArtifactKind::React => "jsx",
            ArtifactKind::Code => code_extension(language.unwrap_or("")),
        }
    }
}

fn code_extension(language: &str) -> &'static str {
    match language.trim().to_ascii_lowercase().as_str() {
        "rust" | "rs" => "rs",
        "python" | "py" => "py",
        "javascript" | "js" => "js",
        "typescript" | "ts" => "ts",
        "tsx" => "tsx",
        "jsx" => "jsx",
        "c" => "c",
        "cpp" | "c++" | "cxx" => "cpp",
        "csharp" | "c#" | "cs" => "cs",
        "java" => "java",
        "kotlin" | "kt" => "kt",
        "go" | "golang" => "go",
        "ruby" | "rb" => "rb",
        "php" => "php",
        "swift" => "swift",
        "shell" | "bash" | "sh" | "zsh" => "sh",
        "powershell" | "ps1" | "pwsh" => "ps1",
        "sql" => "sql",
        "json" => "json",
        "yaml" | "yml" => "yaml",
        "toml" => "toml",
        "xml" => "xml",
        "css" => "css",
        "scss" => "scss",
        "html" => "html",
        "lua" => "lua",
        "r" => "r",
        "dart" => "dart",
        "scala" => "scala",
        "haskell" | "hs" => "hs",
        "dockerfile" => "dockerfile",
        "makefile" | "make" => "mk",
        "markdown" | "md" => "md",
        _ => "txt",
    }
}

/// An artifact: its latest state. `key` is the id the model chose (unique in the chat).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Artifact {
    pub id: String,
    pub conversation_id: String,
    pub key: String,
    pub kind: ArtifactKind,
    pub title: String,
    pub language: Option<String>,
    /// The newest version number (1-based).
    pub version: u32,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// One version of an artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactVersion {
    pub artifact_id: String,
    pub version: u32,
    pub kind: ArtifactKind,
    pub title: String,
    pub language: Option<String>,
    pub content: String,
    /// The reply that made it.
    pub message_id: Option<String>,
    pub created_at: Timestamp,
}

/// An artifact with every version, oldest first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactDetail {
    pub artifact: Artifact,
    pub versions: Vec<ArtifactVersion>,
}

/// A row of the artifacts gallery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactSummary {
    pub artifact: Artifact,
    pub conversation_title: String,
    /// The start of the newest version (for a preview line).
    pub excerpt: String,
}

/// Normalizes the key a model chose: lowercase letters, digits and `-`, at most
/// [`MAX_KEY_CHARS`]. `None` if nothing usable is left.
pub fn clean_key(key: &str) -> Option<String> {
    let mut out = String::new();
    for c in key.trim().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if (c == '-' || c == '_' || c == ' ' || c == '.') && !out.ends_with('-') {
            out.push('-');
        }
        if out.chars().count() >= MAX_KEY_CHARS {
            break;
        }
    }
    let out = out.trim_matches('-').to_owned();
    (!out.is_empty()).then_some(out)
}

/// A key made from a title, when the model gave none.
pub fn key_from_title(title: &str) -> String {
    clean_key(title).unwrap_or_else(|| "artifact".into())
}

/// One line, at most [`MAX_TITLE_CHARS`].
pub fn clean_title(title: &str) -> Option<String> {
    let one = title.split_whitespace().collect::<Vec<_>>().join(" ");
    let one: String = one.chars().take(MAX_TITLE_CHARS).collect();
    (!one.is_empty()).then_some(one)
}

/// A file name for downloading a version: the title made safe, plus the extension.
pub fn file_name(title: &str, kind: ArtifactKind, language: Option<&str>) -> String {
    let mut base: String = title
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' || c == ' ' || c == '.' {
                c
            } else {
                '-'
            }
        })
        .collect();
    base = base.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut base = base.trim_matches(['.', '-', ' ']).to_owned();
    if base.is_empty() {
        base = "artifact".into();
    }
    let base: String = base.chars().take(80).collect();
    format!("{base}.{}", kind.extension(language))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_parse_forgivingly() {
        for k in ArtifactKind::ALL {
            assert_eq!(ArtifactKind::parse(k.as_str()), Some(k));
        }
        assert_eq!(
            ArtifactKind::parse("text/markdown"),
            Some(ArtifactKind::Markdown)
        );
        assert_eq!(
            ArtifactKind::parse("image/svg+xml"),
            Some(ArtifactKind::Svg)
        );
        assert_eq!(ArtifactKind::parse(" JSX "), Some(ArtifactKind::React));
        assert_eq!(ArtifactKind::parse("spreadsheet"), None);
        let json = serde_json::to_value(ArtifactKind::Mermaid).expect("serializes");
        assert_eq!(json, "mermaid");
    }

    #[test]
    fn keys_and_titles_are_cleaned() {
        assert_eq!(clean_key(" Todo App! "), Some("todo-app".into()));
        assert_eq!(clean_key("my_module.rs"), Some("my-module-rs".into()));
        assert_eq!(clean_key("--"), None);
        assert_eq!(clean_key(&"x".repeat(100)).map(|k| k.len()), Some(64));
        assert_eq!(key_from_title("???"), "artifact");
        assert_eq!(clean_title("  A\n title "), Some("A title".into()));
        assert_eq!(clean_title("  "), None);
    }

    #[test]
    fn file_names_are_safe() {
        assert_eq!(
            file_name("Budget: 2026/27", ArtifactKind::Markdown, None),
            "Budget- 2026-27.md"
        );
        assert_eq!(
            file_name("main", ArtifactKind::Code, Some("Rust")),
            "main.rs"
        );
        assert_eq!(file_name("..", ArtifactKind::Html, None), "artifact.html");
        assert_eq!(file_name("x", ArtifactKind::Code, Some("cobol")), "x.txt");
    }
}
