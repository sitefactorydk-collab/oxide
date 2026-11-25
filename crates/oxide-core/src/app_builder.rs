//! The Utility App builder (Phase 21): a chat on the left that builds an app, and the app's
//! Preview and Code on the right.
//!
//! The builder works on a **draft**: a copy of the app's files in `<app data>/app-drafts/<id>/`.
//! The gallery runs the saved app (`<app data>/apps/<id>/`) until **Save to Utility Apps**
//! copies the draft over it. Every model turn and every manual save makes a checkpoint of the
//! draft; **Restore** writes one back.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{AppFile, Timestamp, UtilityApp};

/// Lists the draft's files.
pub const APP_LIST_FILES: &str = "app_list_files";
/// Reads one file of the draft.
pub const APP_READ_FILE: &str = "app_read_file";
/// Creates or replaces a file of the draft.
pub const APP_WRITE_FILE: &str = "app_write_file";
/// Replaces text in a file of the draft.
pub const APP_EDIT_FILE: &str = "app_edit_file";
/// Deletes a file of the draft.
pub const APP_DELETE_FILE: &str = "app_delete_file";
/// Renames (moves) a file of the draft.
pub const APP_RENAME_FILE: &str = "app_rename_file";
/// Changes the draft's manifest (name, description, icon, category, capabilities, models).
pub const APP_UPDATE_MANIFEST: &str = "app_update_manifest";
/// The preview's build errors, runtime errors and console after the last change.
pub const APP_PREVIEW_STATUS: &str = "app_preview_status";
/// A screenshot of the preview.
pub const APP_SCREENSHOT: &str = "app_screenshot";

/// Every builder tool.
pub const APP_BUILDER_TOOLS: [&str; 9] = [
    APP_LIST_FILES,
    APP_READ_FILE,
    APP_WRITE_FILE,
    APP_EDIT_FILE,
    APP_DELETE_FILE,
    APP_RENAME_FILE,
    APP_UPDATE_MANIFEST,
    APP_PREVIEW_STATUS,
    APP_SCREENSHOT,
];

/// Most checkpoints kept per app (the oldest go first).
pub const MAX_APP_CHECKPOINTS: usize = 60;

/// An app open in the builder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppDraft {
    /// The draft's manifest (`folder` is the draft's folder; `problem` says why it can't run).
    pub app: UtilityApp,
    /// The draft's text files (what Preview runs and Code shows).
    pub files: Vec<AppFile>,
    /// A saved copy exists in Utility Apps.
    pub published: bool,
    /// The draft differs from the saved copy (or there is none yet).
    pub unsaved: bool,
    /// The builder chat.
    pub conversation_id: String,
}

/// What made a checkpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum CheckpointKind {
    /// The app as the builder opened it.
    Start,
    /// After a reply of the builder's model.
    Model,
    /// A save in Code.
    Manual,
    /// Before a restore (so a restore can be undone).
    Restore,
}

impl CheckpointKind {
    pub fn as_str(self) -> &'static str {
        match self {
            CheckpointKind::Start => "start",
            CheckpointKind::Model => "model",
            CheckpointKind::Manual => "manual",
            CheckpointKind::Restore => "restore",
        }
    }

    pub fn parse(s: &str) -> CheckpointKind {
        match s {
            "start" => CheckpointKind::Start,
            "model" => CheckpointKind::Model,
            "restore" => CheckpointKind::Restore,
            _ => CheckpointKind::Manual,
        }
    }
}

/// A version of a draft.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppCheckpoint {
    pub id: String,
    pub app_id: String,
    /// What happened (“Start”, “After: make the chart blue”, “Saved App.tsx”).
    pub label: String,
    pub kind: CheckpointKind,
    /// Files in this version.
    pub files: u32,
    pub created_at: Timestamp,
    /// What changed since the checkpoint before it (empty for the first one).
    #[serde(default)]
    pub changes: Vec<CheckpointChange>,
}

/// How one file changed between two checkpoints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum FileChangeKind {
    Added,
    Changed,
    Deleted,
}

/// One file's change between two checkpoints, in lines.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointChange {
    pub path: String,
    pub kind: FileChangeKind,
    pub added: u32,
    pub removed: u32,
}

/// What changed from `before` to `after` (path, content), in path order. Binary files count
/// as one line.
pub fn checkpoint_changes(
    before: &[(String, Vec<u8>)],
    after: &[(String, Vec<u8>)],
) -> Vec<CheckpointChange> {
    use std::collections::BTreeMap;
    let lines = |b: &[u8]| -> u32 {
        match std::str::from_utf8(b) {
            Ok(t) => u32::try_from(t.lines().count()).unwrap_or(u32::MAX),
            Err(_) => 1,
        }
    };
    let old: BTreeMap<&str, &[u8]> = before
        .iter()
        .map(|(p, b)| (p.as_str(), b.as_slice()))
        .collect();
    let new: BTreeMap<&str, &[u8]> = after
        .iter()
        .map(|(p, b)| (p.as_str(), b.as_slice()))
        .collect();
    let mut out = Vec::new();
    for (path, b) in &new {
        match old.get(path) {
            None => out.push(CheckpointChange {
                path: (*path).to_owned(),
                kind: FileChangeKind::Added,
                added: lines(b),
                removed: 0,
            }),
            Some(a) if a != b => {
                let (added, removed) = match (std::str::from_utf8(a), std::str::from_utf8(b)) {
                    (Ok(x), Ok(y)) => crate::edit::line_changes(x, y),
                    _ => (1, 1),
                };
                out.push(CheckpointChange {
                    path: (*path).to_owned(),
                    kind: FileChangeKind::Changed,
                    added: u32::try_from(added).unwrap_or(u32::MAX),
                    removed: u32::try_from(removed).unwrap_or(u32::MAX),
                });
            }
            Some(_) => {}
        }
    }
    for (path, a) in &old {
        if !new.contains_key(path) {
            out.push(CheckpointChange {
                path: (*path).to_owned(),
                kind: FileChangeKind::Deleted,
                added: 0,
                removed: lines(a),
            });
        }
    }
    out.sort_by(|x, y| x.path.cmp(&y.path));
    out
}

/// What the builder asks the open Preview.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum PreviewRequestKind {
    /// Reload the draft, then report build errors, runtime errors and the console.
    Status,
    /// Reload the draft, then take a PNG of the app.
    Screenshot,
}

/// One console line of the preview.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PreviewConsoleLine {
    /// `log`, `info`, `warn` or `error`.
    pub level: String,
    pub text: String,
}

/// The Preview's answer to a [`PreviewRequestKind`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct PreviewReport {
    /// The app compiled and rendered.
    pub rendered: bool,
    /// Compile errors and uncaught runtime errors, oldest first.
    pub errors: Vec<String>,
    /// The newest console lines.
    pub console: Vec<PreviewConsoleLine>,
    /// A screenshot as a `data:image/png;base64,…` URL.
    pub screenshot: Option<String>,
    /// Why the preview couldn't answer (no preview open, the screenshot failed).
    pub note: Option<String>,
}

/// The label of a model-turn checkpoint: the user's request's first line.
pub fn checkpoint_label(request: &str) -> String {
    let first = request.trim().lines().next().unwrap_or("").trim();
    if first.is_empty() {
        return "After a reply".into();
    }
    let cut: String = first.chars().take(60).collect();
    if cut.chars().count() < first.chars().count() {
        format!("After: {}…", cut.trim_end())
    } else {
        format!("After: {cut}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_take_the_first_line_shortened() {
        assert_eq!(
            checkpoint_label("make it blue\nplease"),
            "After: make it blue"
        );
        assert_eq!(checkpoint_label("  "), "After a reply");
        let long = "x".repeat(80);
        assert!(checkpoint_label(&long).ends_with('…'));
        assert_eq!(
            checkpoint_label(&long).chars().count(),
            "After: ".len() + 61
        );
    }

    #[test]
    fn kinds_round_trip() {
        for k in [
            CheckpointKind::Start,
            CheckpointKind::Model,
            CheckpointKind::Manual,
            CheckpointKind::Restore,
        ] {
            assert_eq!(CheckpointKind::parse(k.as_str()), k);
        }
    }
}
