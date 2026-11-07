//! Agentic work in a chat: folders the user granted, the task list, and the names of the
//! built-in tools that use them (Phase 11).

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::Timestamp;

/// Lists a folder.
pub const LIST_DIR: &str = "list_dir";
/// Reads a text file.
pub const READ_FILE: &str = "read_file";
/// Creates or replaces a file.
pub const WRITE_FILE: &str = "write_file";
/// Replaces one passage of a file.
pub const EDIT_FILE: &str = "edit_file";
/// Finds files by name and text.
pub const SEARCH_FILES: &str = "search_files";
/// Moves or renames a file or folder.
pub const MOVE_FILE: &str = "move_file";
/// Deletes a file or folder (by default: moves it into [`TO_DELETE_DIR`]).
pub const DELETE_FILE: &str = "delete_file";
/// Sets the chat's task list.
pub const TASK_LIST: &str = "task_list";
/// Runs a shell command in a granted folder.
pub const RUN_COMMAND: &str = "run_command";

/// `ChatTools::disabled_builtins` entry that switches all file tools off in a chat.
pub const FILES_SWITCH: &str = "files";

/// The file tools, in the order models get them.
pub const FILE_TOOLS: [&str; 7] = [
    LIST_DIR,
    READ_FILE,
    SEARCH_FILES,
    WRITE_FILE,
    EDIT_FILE,
    MOVE_FILE,
    DELETE_FILE,
];

/// Folder (inside each granted folder) that deleted files are moved to, unless the chat
/// allows deleting for good.
pub const TO_DELETE_DIR: &str = "_to_delete";

/// Most folders one chat may be granted.
pub const MAX_CHAT_FOLDERS: usize = 20;
/// Most steps in a task list.
pub const MAX_TASKS: usize = 50;
/// Longest task title, in characters.
pub const MAX_TASK_CHARS: usize = 200;

/// A folder the user gave a chat. Its file tools can only reach paths inside it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ChatFolder {
    /// The folder's full path, resolved (no `..`, no symbolic links).
    pub path: String,
    /// The folder's own name (last part of the path).
    pub name: String,
    pub added_at: Timestamp,
}

/// The folders of a chat and what its file tools may do there.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ChatWorkspace {
    pub folders: Vec<ChatFolder>,
    /// `delete_file` deletes for good. Off: it moves things into `_to_delete`.
    pub allow_delete: bool,
}

/// Where a step of the task list is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum TaskStatus {
    #[default]
    Pending,
    InProgress,
    Done,
}

impl TaskStatus {
    /// Reads a status as models write it (`in_progress`, `in progress`, `completed`, …).
    pub fn parse(s: &str) -> Option<Self> {
        let s: String = s
            .trim()
            .to_ascii_lowercase()
            .chars()
            .filter(|c| c.is_ascii_alphabetic())
            .collect();
        match s.as_str() {
            "pending" | "todo" | "notstarted" | "open" | "waiting" => Some(Self::Pending),
            "inprogress" | "active" | "running" | "doing" | "started" | "current" => {
                Some(Self::InProgress)
            }
            "done" | "completed" | "complete" | "finished" | "success" | "ok" => Some(Self::Done),
            _ => None,
        }
    }
}

/// One step of a chat's task list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TaskItem {
    pub title: String,
    pub status: TaskStatus,
}

/// A chat's task list (the session panel's "Progress").
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TaskList {
    pub conversation_id: String,
    pub items: Vec<TaskItem>,
    pub updated_at: Timestamp,
}

impl TaskList {
    /// Steps done and steps in all.
    pub fn progress(&self) -> (usize, usize) {
        let done = self
            .items
            .iter()
            .filter(|t| t.status == TaskStatus::Done)
            .count();
        (done, self.items.len())
    }

    /// The step being worked on: the first in progress, else the first not done.
    pub fn current(&self) -> Option<(usize, &TaskItem)> {
        self.items
            .iter()
            .enumerate()
            .find(|(_, t)| t.status == TaskStatus::InProgress)
            .or_else(|| {
                self.items
                    .iter()
                    .enumerate()
                    .find(|(_, t)| t.status != TaskStatus::Done)
            })
    }
}

/// A task title as stored: one line, trimmed, at most [`MAX_TASK_CHARS`].
pub fn clean_task_title(title: &str) -> Option<String> {
    let t = title.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.is_empty() {
        None
    } else {
        Some(t.chars().take(MAX_TASK_CHARS).collect())
    }
}

/// Whether a shell command may run without asking: it starts with an allowlist entry, word
/// by word. Commands that chain or redirect (`;`, `&`, `|`, `>`, `<`, a backtick, `$(`)
/// always ask.
pub fn command_allowed(command: &str, allowlist: &[String]) -> bool {
    if command
        .chars()
        .any(|c| matches!(c, ';' | '&' | '|' | '>' | '<' | '`' | '\n' | '\r'))
        || command.contains("$(")
    {
        return false;
    }
    let words: Vec<&str> = command.split_whitespace().collect();
    if words.is_empty() {
        return false;
    }
    allowlist.iter().any(|entry| {
        let want: Vec<&str> = entry.split_whitespace().collect();
        !want.is_empty()
            && want.len() <= words.len()
            && want
                .iter()
                .zip(&words)
                .all(|(a, b)| a.eq_ignore_ascii_case(b))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuses_read_the_way_models_write_them() {
        assert_eq!(
            TaskStatus::parse("in_progress"),
            Some(TaskStatus::InProgress)
        );
        assert_eq!(
            TaskStatus::parse("In Progress"),
            Some(TaskStatus::InProgress)
        );
        assert_eq!(TaskStatus::parse("completed"), Some(TaskStatus::Done));
        assert_eq!(TaskStatus::parse("pending"), Some(TaskStatus::Pending));
        assert_eq!(TaskStatus::parse("maybe"), None);
        let json = serde_json::to_string(&TaskStatus::InProgress).expect("serializes");
        assert_eq!(json, "\"inProgress\"");
    }

    #[test]
    fn progress_and_current_step() {
        let item = |t: &str, s| TaskItem {
            title: t.into(),
            status: s,
        };
        let mut list = TaskList {
            conversation_id: "c".into(),
            items: vec![
                item("a", TaskStatus::Done),
                item("b", TaskStatus::Pending),
                item("c", TaskStatus::InProgress),
            ],
            updated_at: 0,
        };
        assert_eq!(list.progress(), (1, 3));
        assert_eq!(list.current().map(|(i, _)| i), Some(2));
        list.items[2].status = TaskStatus::Done;
        assert_eq!(list.current().map(|(i, _)| i), Some(1));
        list.items[1].status = TaskStatus::Done;
        assert_eq!(list.current(), None);
        assert_eq!(list.progress(), (3, 3));
    }

    #[test]
    fn task_titles_are_one_line() {
        assert_eq!(
            clean_task_title("  Sort\n files ").as_deref(),
            Some("Sort files")
        );
        assert_eq!(clean_task_title("   "), None);
        assert_eq!(
            clean_task_title(&"x".repeat(300)).map(|t| t.chars().count()),
            Some(MAX_TASK_CHARS)
        );
    }

    #[test]
    fn allowlisted_commands_match_word_by_word() {
        let allow = vec!["git status".to_owned(), "dir".to_owned(), "ls".to_owned()];
        assert!(command_allowed("git status -s", &allow));
        assert!(command_allowed("GIT  Status", &allow));
        assert!(!command_allowed("git push", &allow));
        assert!(!command_allowed("git", &allow));
        assert!(command_allowed("ls -la", &allow));
        assert!(!command_allowed("lsblk", &allow));
        assert!(!command_allowed("ls; rm -rf x", &allow));
        assert!(!command_allowed("ls && del x", &allow));
        assert!(!command_allowed("ls > out.txt", &allow));
        assert!(!command_allowed("ls $(whoami)", &allow));
        assert!(!command_allowed("dir | more", &allow));
        assert!(!command_allowed("  ", &allow));
        assert!(!command_allowed("ls", &[]));
    }
}
