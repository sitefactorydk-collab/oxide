//! Paths the user chose (Phase 18's IPC review).
//!
//! Commands that grant a folder to models or install something from disk take a path from
//! the window. The window gets those paths from Oxide's own pickers or from files dropped on
//! it, so Oxide remembers both here, and such a command accepts only a remembered path (or
//! one already granted). Anything else — a window that was somehow made to send its own path
//! — gets a native confirmation dialog the page can't click for the user.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

use crate::error::{CmdResult, CommandError};

/// Most paths kept; older ones are forgotten first (they only skip a confirmation).
const MAX_PATHS: usize = 2_000;

#[derive(Default)]
pub struct ChosenPaths(Mutex<(HashSet<PathBuf>, Vec<PathBuf>)>);

fn normal(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

impl ChosenPaths {
    pub fn add<'a>(&self, paths: impl IntoIterator<Item = &'a Path>) {
        let mut guard = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let (set, order) = &mut *guard;
        for p in paths {
            let n = normal(p);
            if set.insert(n.clone()) {
                order.push(n);
            }
        }
        while order.len() > MAX_PATHS {
            let old = order.remove(0);
            set.remove(&old);
        }
    }

    /// Whether `p` was chosen (a dropped folder also covers the files inside it).
    pub fn contains(&self, p: &Path) -> bool {
        let n = normal(p);
        let guard = self.0.lock().unwrap_or_else(|e| e.into_inner());
        guard.0.iter().any(|c| n == *c || n.starts_with(c))
    }
}

/// Remembers paths returned by a picker or dropped on the main window.
pub fn remember(app: &AppHandle, paths: &[PathBuf]) {
    if let Some(c) = app.try_state::<ChosenPaths>() {
        c.add(paths.iter().map(PathBuf::as_path));
    }
}

/// Lets `path` through when the user chose it; otherwise asks in a native dialog.
/// `what` completes "Oxide was asked to …" (e.g. "give a chat access to this folder").
pub async fn ensure_chosen(app: &AppHandle, path: &Path, what: &str) -> CmdResult<()> {
    if app
        .try_state::<ChosenPaths>()
        .is_some_and(|c| c.contains(path))
    {
        return Ok(());
    }
    tracing::warn!("a path the user didn't pick or drop reached a command; asking");
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .message(format!(
            "Oxide was asked to {what}:\n\n{}\n\nYou didn't pick or drop it in this session. \
             Allow it?",
            path.display()
        ))
        .title("Allow this path?")
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom(
            "Allow".into(),
            "Don't allow".into(),
        ))
        .show(move |ok| {
            let _ = tx.send(ok);
        });
    let ok = rx.await.unwrap_or(false);
    if !ok {
        return Err(CommandError::Invalid("That path wasn't allowed.".into()));
    }
    remember(app, &[path.to_path_buf()]);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chosen_folders_cover_their_contents_only() {
        let dir = std::env::temp_dir().join(format!("oxide-chosen-{}", std::process::id()));
        let inside = dir.join("a");
        let _ = std::fs::create_dir_all(&inside);
        let c = ChosenPaths::default();
        assert!(!c.contains(&dir));
        c.add([dir.as_path()]);
        assert!(c.contains(&dir));
        assert!(c.contains(&inside));
        assert!(!c.contains(dir.parent().expect("parent")));
        let sibling = dir.with_file_name(format!("oxide-chosen-{}-evil", std::process::id()));
        assert!(!c.contains(&sibling));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
