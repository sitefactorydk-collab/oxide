//! Telling which files a script wrote: a cheap snapshot of a folder before and after.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// How deep the snapshot looks.
const MAX_DEPTH: usize = 3;
/// How many files it keeps track of (the rest of a huge folder is not seen).
const MAX_FILES: usize = 5_000;

/// Names the snapshot skips.
fn skipped(name: &str) -> bool {
    name.starts_with('.')
        || matches!(
            name,
            "__pycache__" | "node_modules" | "_to_delete" | "venv" | "target"
        )
}

/// Files of a folder with their size and modified time.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    files: BTreeMap<PathBuf, (u64, Option<SystemTime>)>,
}

/// A file a script created or changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedFile {
    pub path: PathBuf,
    /// Path relative to the folder, with `/` separators.
    pub rel: String,
    pub size: u64,
}

impl Snapshot {
    /// Blocking: call from `spawn_blocking`.
    pub fn take(dir: &Path) -> Self {
        let mut files = BTreeMap::new();
        walk(dir, 0, &mut files);
        Self { files }
    }

    /// Files that are new or changed since `self`, oldest change first.
    pub fn changed_since(&self, dir: &Path, after: &Snapshot) -> Vec<ChangedFile> {
        let mut out: Vec<(Option<SystemTime>, ChangedFile)> = after
            .files
            .iter()
            .filter(|(p, v)| self.files.get(*p) != Some(v))
            .map(|(p, (size, modified))| {
                let rel = p
                    .strip_prefix(dir)
                    .unwrap_or(p)
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/");
                (
                    *modified,
                    ChangedFile {
                        path: p.clone(),
                        rel,
                        size: *size,
                    },
                )
            })
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.rel.cmp(&b.1.rel)));
        out.into_iter().map(|(_, f)| f).collect()
    }
}

fn walk(dir: &Path, depth: usize, files: &mut BTreeMap<PathBuf, (u64, Option<SystemTime>)>) {
    if depth > MAX_DEPTH || files.len() >= MAX_FILES {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        if files.len() >= MAX_FILES {
            return;
        }
        let name = e.file_name().to_string_lossy().into_owned();
        if skipped(&name) {
            continue;
        }
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_dir() {
            walk(&e.path(), depth + 1, files);
        } else if ft.is_file()
            && let Ok(m) = e.metadata()
        {
            files.insert(e.path(), (m.len(), m.modified().ok()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_new_and_changed_files_and_skips_hidden_ones() {
        let tmp = tempfile::tempdir().expect("tmp");
        let d = tmp.path();
        std::fs::write(d.join("keep.txt"), "one").expect("write");
        std::fs::create_dir(d.join("sub")).expect("mkdir");
        std::fs::create_dir(d.join("__pycache__")).expect("mkdir");
        let before = Snapshot::take(d);
        std::fs::write(d.join("keep.txt"), "changed!").expect("write");
        std::fs::write(d.join("sub").join("new.csv"), "a,b").expect("write");
        std::fs::write(d.join("__pycache__").join("x.pyc"), "x").expect("write");
        std::fs::write(d.join(".hidden"), "x").expect("write");
        let after = Snapshot::take(d);
        let changed = before.changed_since(d, &after);
        let rels: Vec<&str> = changed.iter().map(|c| c.rel.as_str()).collect();
        assert!(
            rels.contains(&"keep.txt") && rels.contains(&"sub/new.csv"),
            "{rels:?}"
        );
        assert_eq!(rels.len(), 2, "{rels:?}");
    }
}
