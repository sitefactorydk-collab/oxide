//! Permission modes (Phase 23): Manual asks as before; Auto works on its own and stops to
//! ask when an action looks risky.

use serde::{Deserialize, Serialize};
use specta::Type;

/// How a chat handles tool calls that would ask for approval.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum PermissionMode {
    /// Oxide asks before tools that change things (Allow once / Allow always / Deny).
    #[default]
    Manual,
    /// Oxide works on its own: reads, changes inside shared folders and what was allowed run
    /// at once; a few things always ask; the rest goes to the safety check, which stops to
    /// ask when an action looks risky.
    Auto,
}

impl PermissionMode {
    /// The value stored in the database.
    pub fn as_str(self) -> &'static str {
        match self {
            PermissionMode::Manual => "manual",
            PermissionMode::Auto => "auto",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "auto" => PermissionMode::Auto,
            _ => PermissionMode::Manual,
        }
    }
}

/// Why a tool call ran (or didn't) without a card in Auto: the small **Auto** mark on its line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum AutoMark {
    /// The tool only reads or searches.
    ReadOnly,
    /// A change inside a folder shared with the chat (or a copy between it and the sandbox).
    InFolder,
    /// Inside the chat's Docker sandbox, which has no network.
    Sandboxed,
    /// The user allowed this tool for the chat (Allow always).
    Allowed,
    /// The safety check looked at it and let it run.
    Checked,
    /// The safety check stopped it and asked the user (whatever they answered).
    Stopped,
}

impl AutoMark {
    /// The value stored in the database.
    pub fn as_str(self) -> &'static str {
        match self {
            AutoMark::ReadOnly => "readOnly",
            AutoMark::InFolder => "inFolder",
            AutoMark::Sandboxed => "sandboxed",
            AutoMark::Allowed => "allowed",
            AutoMark::Checked => "checked",
            AutoMark::Stopped => "stopped",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "readOnly" => AutoMark::ReadOnly,
            "inFolder" => AutoMark::InFolder,
            "sandboxed" => AutoMark::Sandboxed,
            "allowed" => AutoMark::Allowed,
            "checked" => AutoMark::Checked,
            "stopped" => AutoMark::Stopped,
            _ => return None,
        })
    }

    /// Ran on its own (everything but [`AutoMark::Stopped`]).
    pub fn ran_automatically(self) -> bool {
        self != AutoMark::Stopped
    }
}

/// Most entries in Settings → Permissions → Always ask.
pub const MAX_ALWAYS_ASK: usize = 200;

/// Whether a tool matches an "always ask" entry: the tool's name (`run_command`), a connector
/// tool as `server/tool`, or a whole connector as `server` (case doesn't matter).
pub fn always_ask_matches(entries: &[String], server: Option<&str>, tool: &str) -> bool {
    entries.iter().any(|e| {
        let e = e.trim();
        match (server, e.split_once('/')) {
            (Some(s), Some((es, et))) => {
                es.eq_ignore_ascii_case(s) && et.eq_ignore_ascii_case(tool)
            }
            (Some(s), None) => e.eq_ignore_ascii_case(s) || e.eq_ignore_ascii_case(tool),
            (None, Some((es, et))) => {
                es.eq_ignore_ascii_case("builtin") && et.eq_ignore_ascii_case(tool)
            }
            (None, None) => e.eq_ignore_ascii_case(tool),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_and_marks_round_trip() {
        for m in [PermissionMode::Manual, PermissionMode::Auto] {
            assert_eq!(PermissionMode::parse(m.as_str()), m);
        }
        assert_eq!(PermissionMode::parse("??"), PermissionMode::Manual);
        assert_eq!(
            serde_json::to_string(&PermissionMode::Auto).expect("json"),
            "\"auto\""
        );
        for m in [
            AutoMark::ReadOnly,
            AutoMark::InFolder,
            AutoMark::Sandboxed,
            AutoMark::Allowed,
            AutoMark::Checked,
            AutoMark::Stopped,
        ] {
            assert_eq!(AutoMark::parse(m.as_str()), Some(m));
        }
        assert!(!AutoMark::Stopped.ran_automatically());
    }

    #[test]
    fn always_ask_entries_match_tools_and_connectors() {
        let list = vec![
            "run_command".to_owned(),
            "github/create_issue".to_owned(),
            "Slack".to_owned(),
        ];
        assert!(always_ask_matches(&list, None, "run_command"));
        assert!(!always_ask_matches(&list, None, "run_python"));
        assert!(always_ask_matches(&list, Some("github"), "create_issue"));
        assert!(!always_ask_matches(&list, Some("github"), "list_issues"));
        assert!(always_ask_matches(&list, Some("slack"), "post_message"));
        assert!(always_ask_matches(
            &["builtin/write_file".to_owned()],
            None,
            "write_file"
        ));
    }
}
