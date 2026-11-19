//! File tools confined to the folders the user granted a chat (Phase 11).
//!
//! Every path a model passes goes through [`Sandbox::resolve`]: the longest part of it that
//! exists is resolved by the OS (`..`, symbolic links, junctions, letter case), the rest is
//! appended without `..`, and the result must lie inside a granted folder (itself resolved
//! the same way). A link inside a granted folder that points outside it is refused like any
//! other path outside.
//!
//! The functions here do blocking I/O; callers run them in `spawn_blocking`.

use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use oxide_core::workspace::{
    DELETE_FILE, EDIT_FILE, LIST_DIR, MOVE_FILE, READ_FILE, SEARCH_FILES, TO_DELETE_DIR, WRITE_FILE,
};
use oxide_core::{ChatFolder, ChatWorkspace};
use serde_json::{Value, json};

use super::{Builtin, ToolEntry, ToolKind};

/// Largest file `read_file` reads.
pub const MAX_READ_BYTES: u64 = 10 * 1024 * 1024;
/// Most characters `read_file` returns in one call.
pub const MAX_READ_CHARS: usize = 60_000;
/// Largest file `write_file` writes.
pub const MAX_WRITE_BYTES: usize = 5 * 1024 * 1024;
/// Most entries `list_dir` returns.
pub const MAX_LIST_ENTRIES: usize = 1_000;
/// Most files `search_files` looks at.
pub const MAX_SEARCH_FILES: usize = 20_000;
/// Most matches `search_files` returns.
pub const MAX_SEARCH_MATCHES: usize = 200;
/// Largest file whose text `search_files` reads.
const MAX_SEARCH_BYTES: u64 = 2 * 1024 * 1024;

/// One file operation a model asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileOp {
    List,
    Read,
    Search,
    Write,
    Edit,
    Move,
    Delete,
}

impl FileOp {
    pub const ALL: [FileOp; 7] = [
        FileOp::List,
        FileOp::Read,
        FileOp::Search,
        FileOp::Write,
        FileOp::Edit,
        FileOp::Move,
        FileOp::Delete,
    ];

    pub fn name(self) -> &'static str {
        match self {
            FileOp::List => LIST_DIR,
            FileOp::Read => READ_FILE,
            FileOp::Search => SEARCH_FILES,
            FileOp::Write => WRITE_FILE,
            FileOp::Edit => EDIT_FILE,
            FileOp::Move => MOVE_FILE,
            FileOp::Delete => DELETE_FILE,
        }
    }

    /// Only looks: runs without asking (the user granted the folder).
    pub fn read_only(self) -> bool {
        matches!(self, FileOp::List | FileOp::Read | FileOp::Search)
    }
}

/// Removes Windows' `\\?\` prefix that `canonicalize` adds, for paths shown to people and
/// models. `\\?\UNC\server\share` becomes `\\server\share`.
pub fn display_path(p: &Path) -> String {
    let s = p.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        rest.to_owned()
    } else {
        s.into_owned()
    }
}

/// Resolves a folder the user picked, for granting: it must exist and be a folder.
pub fn resolve_grant(path: &str) -> Result<String, String> {
    let p = path.trim();
    if p.is_empty() {
        return Err("Pick a folder.".into());
    }
    let pb = PathBuf::from(p);
    if !pb.is_absolute() {
        return Err(format!("“{p}” isn't a full path."));
    }
    let real = fs::canonicalize(&pb).map_err(|e| format!("“{p}” can't be opened: {e}"))?;
    if !real.is_dir() {
        return Err(format!("“{p}” isn't a folder."));
    }
    Ok(display_path(&real))
}

/// The granted folders of one chat, resolved.
#[derive(Debug, Clone, Default)]
pub struct Sandbox {
    /// (resolved path, folder name as shown).
    roots: Vec<(PathBuf, String)>,
    allow_delete: bool,
}

impl Sandbox {
    /// Folders that no longer exist are left out.
    pub fn new(workspace: &ChatWorkspace) -> Self {
        Self::from_folders(&workspace.folders, workspace.allow_delete)
    }

    pub fn from_folders(folders: &[ChatFolder], allow_delete: bool) -> Self {
        let roots = folders
            .iter()
            .filter_map(|f| {
                let real = fs::canonicalize(&f.path).ok()?;
                real.is_dir().then(|| (real, f.name.clone()))
            })
            .collect();
        Self {
            roots,
            allow_delete,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.roots.is_empty()
    }

    /// `delete_file` deletes for good (else it moves things into `_to_delete`).
    pub fn allows_delete(&self) -> bool {
        self.allow_delete
    }

    /// The folders' paths as shown.
    pub fn root_paths(&self) -> Vec<String> {
        self.roots.iter().map(|(p, _)| display_path(p)).collect()
    }

    fn root_of(&self, p: &Path) -> Option<&Path> {
        self.roots
            .iter()
            .map(|(r, _)| r.as_path())
            .filter(|r| p.starts_with(r))
            .max_by_key(|r| r.components().count())
    }

    /// Resolves a path a model passed. Relative paths are taken from the first folder, or
    /// from the folder whose name they start with. The result may not exist yet (for
    /// writes), but its existing part is resolved and it lies inside a granted folder.
    pub fn resolve(&self, raw: &str) -> Result<PathBuf, String> {
        let Some((first, _)) = self.roots.first() else {
            return Err("No folders are shared with this chat. Ask the user to add one.".into());
        };
        let raw = raw.trim();
        let raw = if raw.is_empty() { "." } else { raw };
        let given = PathBuf::from(raw);
        let joined = if given.is_absolute() {
            given
        } else {
            // "Photos/a.jpg" with a granted folder named Photos means that folder.
            let mut comps = given.components();
            let named = match comps.next() {
                Some(Component::Normal(c)) => self
                    .roots
                    .iter()
                    .find(|(r, name)| {
                        name.eq_ignore_ascii_case(&c.to_string_lossy()) && !r.join(&given).exists()
                    })
                    .map(|(r, _)| r.join(comps.as_path())),
                _ => None,
            };
            named.unwrap_or_else(|| first.join(&given))
        };
        let resolved = resolve_existing_prefix(&joined)?;
        if self.root_of(&resolved).is_none() {
            return Err(format!(
                "“{raw}” is outside the folders shared with this chat ({}). Only paths inside \
                 them can be used.",
                self.root_paths().join(", ")
            ));
        }
        Ok(resolved)
    }

    /// Like [`Sandbox::resolve`], but the path must exist.
    fn existing(&self, raw: &str) -> Result<PathBuf, String> {
        let p = self.resolve(raw)?;
        if fs::symlink_metadata(&p).is_err() {
            return Err(format!("“{}” doesn't exist.", display_path(&p)));
        }
        Ok(p)
    }

    /// Paths that may change (move, delete, write): not a granted folder itself.
    fn changeable(&self, p: &Path) -> Result<(), String> {
        if self.roots.iter().any(|(r, _)| r == p) {
            return Err(format!(
                "“{}” is a shared folder itself; it can't be moved, replaced or deleted.",
                display_path(p)
            ));
        }
        Ok(())
    }

    /// Runs one operation.
    pub fn run(&self, op: FileOp, input: &Value) -> Result<String, String> {
        match op {
            FileOp::List => self.list_dir(input),
            FileOp::Read => self.read_file(input),
            FileOp::Search => self.search(input),
            FileOp::Write => self.write_file(input),
            FileOp::Edit => self.edit_file(input),
            FileOp::Move => self.move_file(input),
            FileOp::Delete => self.delete(input),
        }
    }

    fn list_dir(&self, input: &Value) -> Result<String, String> {
        let dir = self.existing(&str_arg(input, "path").unwrap_or_default())?;
        if !dir.is_dir() {
            return Err(format!("“{}” is a file, not a folder.", display_path(&dir)));
        }
        let depth = input
            .get("depth")
            .and_then(Value::as_u64)
            .unwrap_or(1)
            .clamp(1, 3);
        let mut lines = Vec::new();
        let mut cut = false;
        self.list_into(&dir, "", depth, &mut lines, &mut cut)?;
        let mut out = format!("{}\n", display_path(&dir));
        if lines.is_empty() {
            out.push_str("(empty)");
        } else {
            out.push_str(&lines.join("\n"));
        }
        if cut {
            out.push_str(&format!(
                "\n[Only the first {MAX_LIST_ENTRIES} entries are shown.]"
            ));
        }
        Ok(out)
    }

    fn list_into(
        &self,
        dir: &Path,
        indent: &str,
        depth: u64,
        lines: &mut Vec<String>,
        cut: &mut bool,
    ) -> Result<(), String> {
        let mut entries: Vec<(bool, String, u64, PathBuf)> = fs::read_dir(dir)
            .map_err(|e| format!("“{}” can't be listed: {e}", display_path(dir)))?
            .filter_map(Result::ok)
            .map(|e| {
                let meta = fs::symlink_metadata(e.path()).ok();
                let is_dir = e.path().is_dir();
                let size = meta.map(|m| m.len()).unwrap_or(0);
                (
                    is_dir,
                    e.file_name().to_string_lossy().into_owned(),
                    size,
                    e.path(),
                )
            })
            .collect();
        entries.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| a.1.to_lowercase().cmp(&b.1.to_lowercase()))
        });
        for (is_dir, name, size, path) in entries {
            if lines.len() >= MAX_LIST_ENTRIES {
                *cut = true;
                return Ok(());
            }
            if is_dir {
                lines.push(format!("{indent}{name}/"));
                // Only descend into folders that stay inside the grant (not links out).
                let inside = fs::canonicalize(&path)
                    .ok()
                    .is_some_and(|real| self.root_of(&real).is_some());
                if depth > 1 && inside {
                    self.list_into(&path, &format!("{indent}  "), depth - 1, lines, cut)?;
                }
            } else {
                lines.push(format!("{indent}{name}  ({})", human_size(size)));
            }
        }
        Ok(())
    }

    fn read_file(&self, input: &Value) -> Result<String, String> {
        let path = self.existing(&required(input, "path", READ_FILE)?)?;
        if path.is_dir() {
            return Err(format!(
                "“{}” is a folder; use list_dir.",
                display_path(&path)
            ));
        }
        let size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        if size > MAX_READ_BYTES {
            return Err(format!(
                "“{}” is {}, too large to read (at most {}).",
                display_path(&path),
                human_size(size),
                human_size(MAX_READ_BYTES)
            ));
        }
        let bytes = fs::read(&path).map_err(|e| format!("The file can't be read: {e}"))?;
        let text = match String::from_utf8(bytes) {
            Ok(t) => t,
            Err(e) if looks_binary(e.as_bytes()) => {
                return Err(format!(
                    "“{}” ({}) isn't a text file; its contents can't be shown.",
                    display_path(&path),
                    human_size(size)
                ));
            }
            Err(e) => String::from_utf8_lossy(e.as_bytes()).into_owned(),
        };
        let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
        let lines: Vec<&str> = text.lines().collect();
        let total = lines.len();
        let start = input
            .get("start_line")
            .and_then(Value::as_u64)
            .map(|n| usize::try_from(n).unwrap_or(usize::MAX))
            .unwrap_or(1)
            .max(1);
        let want = input
            .get("max_lines")
            .and_then(Value::as_u64)
            .map(|n| usize::try_from(n).unwrap_or(usize::MAX))
            .unwrap_or(usize::MAX)
            .max(1);
        if start > total.max(1) {
            return Err(format!("The file has only {total} lines."));
        }
        let mut out = String::new();
        let mut shown = 0usize;
        for line in lines.iter().skip(start - 1).take(want) {
            if out.chars().count() + line.chars().count() + 1 > MAX_READ_CHARS && shown > 0 {
                break;
            }
            out.push_str(line);
            out.push('\n');
            shown += 1;
        }
        let end = start - 1 + shown;
        let mut head = format!("{} ({})", display_path(&path), human_size(size));
        if start > 1 || end < total {
            head.push_str(&format!(", lines {start}–{end} of {total}"));
        }
        let mut res = format!("{head}\n\n{out}");
        if end < total {
            res.push_str(&format!(
                "\n[{} more lines. Call read_file with start_line {} to read on.]",
                total - end,
                end + 1
            ));
        }
        Ok(res)
    }

    fn write_file(&self, input: &Value) -> Result<String, String> {
        let path = self.resolve(&required(input, "path", WRITE_FILE)?)?;
        self.changeable(&path)?;
        let content = input
            .get("content")
            .and_then(Value::as_str)
            .ok_or("write_file needs a \"content\" string.")?;
        if content.len() > MAX_WRITE_BYTES {
            return Err(format!(
                "The content is too large ({}; at most {}).",
                human_size(content.len() as u64),
                human_size(MAX_WRITE_BYTES as u64)
            ));
        }
        if path.is_dir() {
            return Err(format!("“{}” is a folder.", display_path(&path)));
        }
        let existed = path.exists();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("The folder can't be made: {e}"))?;
        }
        fs::write(&path, content).map_err(|e| format!("The file can't be written: {e}"))?;
        Ok(format!(
            "{} {} ({}).",
            if existed { "Replaced" } else { "Created" },
            display_path(&path),
            human_size(content.len() as u64)
        ))
    }

    fn edit_file(&self, input: &Value) -> Result<String, String> {
        let path = self.existing(&required(input, "path", EDIT_FILE)?)?;
        let old = input
            .get("old_str")
            .and_then(Value::as_str)
            .ok_or("edit_file needs an \"old_str\" string.")?;
        let new = input.get("new_str").and_then(Value::as_str).unwrap_or("");
        if path.is_dir() {
            return Err(format!("“{}” is a folder.", display_path(&path)));
        }
        let size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        if size > MAX_WRITE_BYTES as u64 {
            return Err(format!("“{}” is too large to edit.", display_path(&path)));
        }
        let text = fs::read_to_string(&path)
            .map_err(|e| format!("The file can't be read as text: {e}"))?;
        let next = oxide_core::artifact::apply_update(&text, old, new).map_err(|e| {
            use oxide_core::artifact::UpdateError;
            match e {
                UpdateError::EmptyOld => "old_str is empty.".to_owned(),
                UpdateError::NotFound => {
                    "old_str wasn't found in the file. Read the file and copy the passage \
                     exactly."
                        .to_owned()
                }
                UpdateError::Ambiguous(n) => format!(
                    "old_str appears {n} times; include more surrounding text so it's unique."
                ),
            }
        })?;
        fs::write(&path, &next).map_err(|e| format!("The file can't be written: {e}"))?;
        Ok(format!("Edited {}.", display_path(&path)))
    }

    fn move_file(&self, input: &Value) -> Result<String, String> {
        let from_raw = str_arg(input, "from")
            .or_else(|| str_arg(input, "source"))
            .or_else(|| str_arg(input, "path"))
            .ok_or("move_file needs \"from\" and \"to\" paths.")?;
        let to_raw = str_arg(input, "to")
            .or_else(|| str_arg(input, "destination"))
            .ok_or("move_file needs \"from\" and \"to\" paths.")?;
        let from = self.existing(&from_raw)?;
        self.changeable(&from)?;
        let mut to = self.resolve(&to_raw)?;
        // Moving into an existing folder keeps the name.
        if to.is_dir()
            && let Some(name) = from.file_name()
        {
            to = to.join(name);
        }
        self.changeable(&to)?;
        if to.starts_with(&from) {
            return Err("A folder can't be moved into itself.".into());
        }
        if fs::symlink_metadata(&to).is_ok() {
            return Err(format!(
                "“{}” already exists; pick another name.",
                display_path(&to)
            ));
        }
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("The folder can't be made: {e}"))?;
        }
        fs::rename(&from, &to).map_err(|e| format!("It couldn't be moved: {e}"))?;
        Ok(format!(
            "Moved {} → {}.",
            display_path(&from),
            display_path(&to)
        ))
    }

    fn delete(&self, input: &Value) -> Result<String, String> {
        let path = self.existing(&required(input, "path", DELETE_FILE)?)?;
        self.changeable(&path)?;
        let is_dir = path.is_dir();
        if self.allow_delete {
            if is_dir {
                fs::remove_dir_all(&path)
            } else {
                fs::remove_file(&path)
            }
            .map_err(|e| format!("It couldn't be deleted: {e}"))?;
            return Ok(format!("Deleted {} for good.", display_path(&path)));
        }
        let root = self
            .root_of(&path)
            .ok_or("The path is outside the shared folders.")?
            .to_path_buf();
        let bin = root.join(TO_DELETE_DIR);
        if path.starts_with(&bin) {
            return Err(format!(
                "“{}” is already in {TO_DELETE_DIR}. The user can empty that folder, or allow \
                 deleting for good in the chat's session panel.",
                display_path(&path)
            ));
        }
        let rel = path.strip_prefix(&root).unwrap_or(&path);
        let mut target = bin.join(rel);
        if fs::symlink_metadata(&target).is_ok() {
            target = unique_sibling(&target);
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("{TO_DELETE_DIR} can't be made: {e}"))?;
        }
        fs::rename(&path, &target).map_err(|e| format!("It couldn't be moved away: {e}"))?;
        Ok(format!(
            "Moved {} to {} (deleting for good isn't allowed in this chat; the user can empty \
             {TO_DELETE_DIR} later).",
            display_path(&path),
            display_path(&target)
        ))
    }

    fn search(&self, input: &Value) -> Result<String, String> {
        let base = self.existing(&str_arg(input, "path").unwrap_or_default())?;
        let name = str_arg(input, "name");
        let text = str_arg(input, "text").or_else(|| str_arg(input, "query"));
        if name.is_none() && text.is_none() {
            return Err(
                "search_files needs a \"name\" pattern, a \"text\" to find, or both.".into(),
            );
        }
        let pattern = name.as_deref().map(|n| {
            if n.contains('*') || n.contains('?') {
                n.to_lowercase()
            } else {
                format!("*{}*", n.to_lowercase())
            }
        });
        let needle = text.as_deref().map(str::to_lowercase);
        let mut matches: Vec<String> = Vec::new();
        let mut seen = 0usize;
        let mut stack = vec![base.clone()];
        let mut cut = false;
        while let Some(dir) = stack.pop() {
            let Ok(rd) = fs::read_dir(&dir) else { continue };
            let mut entries: Vec<_> = rd.filter_map(Result::ok).collect();
            entries.sort_by_key(|e| e.file_name());
            for e in entries {
                let p = e.path();
                let fname = e.file_name().to_string_lossy().into_owned();
                if p.is_dir() {
                    if fname == ".git" || fname == TO_DELETE_DIR {
                        continue;
                    }
                    if fs::canonicalize(&p)
                        .ok()
                        .is_some_and(|real| self.root_of(&real).is_some())
                    {
                        stack.push(p);
                    }
                    continue;
                }
                // A link to a file outside the grant isn't searched.
                if fs::symlink_metadata(&p).is_ok_and(|m| m.file_type().is_symlink())
                    && !fs::canonicalize(&p)
                        .ok()
                        .is_some_and(|real| self.root_of(&real).is_some())
                {
                    continue;
                }
                seen += 1;
                if seen > MAX_SEARCH_FILES || matches.len() >= MAX_SEARCH_MATCHES {
                    cut = true;
                    break;
                }
                if let Some(pat) = &pattern
                    && !wildcard(pat, &fname.to_lowercase())
                {
                    continue;
                }
                match &needle {
                    None => matches.push(display_path(&p)),
                    Some(n) => {
                        for (no, line) in text_lines(&p) {
                            if line.to_lowercase().contains(n.as_str()) {
                                let shown: String = line.trim().chars().take(200).collect();
                                matches.push(format!("{}:{no}: {shown}", display_path(&p)));
                                if matches.len() >= MAX_SEARCH_MATCHES {
                                    break;
                                }
                            }
                        }
                    }
                }
            }
            if cut {
                break;
            }
        }
        let mut out = if matches.is_empty() {
            format!("Nothing found in {}.", display_path(&base))
        } else {
            format!(
                "{} match{} in {}:\n{}",
                matches.len(),
                if matches.len() == 1 { "" } else { "es" },
                display_path(&base),
                matches.join("\n")
            )
        };
        if cut {
            out.push_str("\n[The search stopped early; narrow it with \"path\" or \"name\".]");
        }
        Ok(out)
    }
}

/// Resolves the longest existing part of `p` and appends the rest. `..` after the
/// existing part is refused (it could step out once the folders exist).
fn resolve_existing_prefix(p: &Path) -> Result<PathBuf, String> {
    let mut existing = p.to_path_buf();
    let mut rest: Vec<std::ffi::OsString> = Vec::new();
    loop {
        if fs::symlink_metadata(&existing).is_ok() {
            break;
        }
        match (existing.file_name(), existing.parent()) {
            (Some(name), Some(parent)) => {
                rest.push(name.to_owned());
                existing = parent.to_path_buf();
            }
            _ => {
                // `..` or a root: let canonicalize fail below with a clear error.
                if existing.ends_with("..") {
                    return Err(format!("“{}” can't be used (“..”).", p.display()));
                }
                break;
            }
        }
    }
    let mut real = fs::canonicalize(&existing)
        .map_err(|e| format!("“{}” can't be opened: {e}", p.display()))?;
    for part in rest.iter().rev() {
        let comp = Path::new(part);
        if comp.components().any(|c| c == Component::ParentDir) {
            return Err(format!("“{}” can't be used (“..”).", p.display()));
        }
        real.push(part);
    }
    Ok(real)
}

fn unique_sibling(p: &Path) -> PathBuf {
    let stem = p
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = p
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    for n in 2.. {
        let candidate = p.with_file_name(format!("{stem} ({n}){ext}"));
        if fs::symlink_metadata(&candidate).is_err() {
            return candidate;
        }
    }
    p.to_path_buf()
}

/// The lines of a text file (1-based numbers); nothing for binary or large files.
fn text_lines(p: &Path) -> Vec<(usize, String)> {
    let Ok(meta) = fs::metadata(p) else {
        return Vec::new();
    };
    if meta.len() > MAX_SEARCH_BYTES {
        return Vec::new();
    }
    let mut buf = Vec::new();
    if fs::File::open(p)
        .and_then(|mut f| f.read_to_end(&mut buf))
        .is_err()
        || looks_binary(&buf)
    {
        return Vec::new();
    }
    String::from_utf8_lossy(&buf)
        .lines()
        .enumerate()
        .map(|(i, l)| (i + 1, l.to_owned()))
        .collect()
}

fn looks_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8_000).any(|b| *b == 0)
}

/// `*` and `?` wildcards (both lowercase already).
fn wildcard(pattern: &str, name: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let n: Vec<char> = name.chars().collect();
    let (mut pi, mut ni) = (0usize, 0usize);
    let (mut star, mut mark) = (None, 0usize);
    while ni < n.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == n[ni]) {
            pi += 1;
            ni += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ni;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ni = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

pub fn human_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    let b = bytes as f64;
    if b < KB {
        format!("{bytes} B")
    } else if b < KB * KB {
        format!("{:.1} KB", b / KB)
    } else if b < KB * KB * KB {
        format!("{:.1} MB", b / KB / KB)
    } else {
        format!("{:.1} GB", b / KB / KB / KB)
    }
}

fn str_arg(input: &Value, key: &str) -> Option<String> {
    input
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

fn required(input: &Value, key: &str, tool: &str) -> Result<String, String> {
    str_arg(input, key).ok_or_else(|| format!("{tool} needs a \"{key}\" string."))
}

/// The usage-log label of a file call: its path, or `from → to` for moves.
pub fn file_label(op: FileOp, input: &Value) -> Option<String> {
    match op {
        FileOp::Move => {
            let from = str_arg(input, "from").or_else(|| str_arg(input, "source"))?;
            let to = str_arg(input, "to").or_else(|| str_arg(input, "destination"))?;
            Some(format!("{from} → {to}"))
        }
        FileOp::Search => str_arg(input, "text")
            .or_else(|| str_arg(input, "query"))
            .or_else(|| str_arg(input, "name")),
        _ => str_arg(input, "path").or_else(|| Some(".".into())),
    }
}

/// The tool as models see it.
pub fn file_entry(op: FileOp, delete_for_good: bool) -> ToolEntry {
    let path = json!({ "type": "string",
        "description": "A full path inside a shared folder, or a path relative to the first one." });
    let (title, description, schema) = match op {
        FileOp::List => (
            "List folder",
            "List the files and folders in a shared folder (folders end with /, files show \
             their size). `depth` 2 or 3 also lists subfolders.",
            json!({"type": "object", "properties": {
                "path": path,
                "depth": {"type": "integer", "minimum": 1, "maximum": 3,
                          "description": "How many levels to list (default 1)."}
            }}),
        ),
        FileOp::Read => (
            "Read file",
            "Read a text file in a shared folder. Long files are cut; pass start_line to \
             read on.",
            json!({"type": "object", "properties": {
                "path": path,
                "start_line": {"type": "integer", "minimum": 1},
                "max_lines": {"type": "integer", "minimum": 1}
            }, "required": ["path"]}),
        ),
        FileOp::Search => (
            "Search files",
            "Find files in a shared folder by name (wildcards * and ?) and/or by text they \
             contain (case-insensitive). Returns paths, and line numbers for text matches.",
            json!({"type": "object", "properties": {
                "path": {"type": "string", "description": "Folder to search (default: the first shared folder)."},
                "name": {"type": "string", "description": "File name or pattern, e.g. *.pdf"},
                "text": {"type": "string", "description": "Text to find inside files."}
            }}),
        ),
        FileOp::Write => (
            "Write file",
            "Create a text file in a shared folder, or replace one, with the given content. \
             Missing folders are made. Prefer edit_file for small changes to existing files.",
            json!({"type": "object", "properties": {
                "path": path,
                "content": {"type": "string", "description": "The whole new content."}
            }, "required": ["path", "content"]}),
        ),
        FileOp::Edit => (
            "Edit file",
            "Replace one passage of a text file in a shared folder: old_str must appear \
             exactly once.",
            json!({"type": "object", "properties": {
                "path": path,
                "old_str": {"type": "string", "description": "The exact text to replace."},
                "new_str": {"type": "string", "description": "The text to put in its place."}
            }, "required": ["path", "old_str", "new_str"]}),
        ),
        FileOp::Move => (
            "Move file",
            "Move or rename a file or folder inside the shared folders. Moving into an \
             existing folder keeps the name; missing folders on the way are made. Existing \
             files are never overwritten.",
            json!({"type": "object", "properties": {
                "from": {"type": "string", "description": "What to move."},
                "to": {"type": "string", "description": "The new path, or a folder to move it into."}
            }, "required": ["from", "to"]}),
        ),
        FileOp::Delete => (
            "Delete file",
            if delete_for_good {
                "Delete a file or folder in a shared folder for good. Only when the user asks."
            } else {
                "Delete a file or folder in a shared folder: it's moved into the shared \
                 folder's _to_delete folder, which the user can empty. Only when the user asks."
            },
            json!({"type": "object", "properties": { "path": path }, "required": ["path"]}),
        ),
    };
    ToolEntry {
        api_name: op.name().into(),
        kind: ToolKind::Builtin(Builtin::File(op)),
        title: Some(title.into()),
        description: description.into(),
        schema,
        read_only: op.read_only(),
        destructive: op == FileOp::Delete || op == FileOp::Write,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sandbox(dirs: &[&Path], allow_delete: bool) -> Sandbox {
        let folders: Vec<ChatFolder> = dirs
            .iter()
            .map(|d| ChatFolder {
                path: resolve_grant(&d.to_string_lossy()).expect("grant"),
                name: d
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                added_at: 0,
            })
            .collect();
        Sandbox::from_folders(&folders, allow_delete)
    }

    fn setup() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let tmp = tempfile::tempdir().expect("tmp");
        let root = tmp.path().join("work");
        let outside = tmp.path().join("secret");
        fs::create_dir_all(root.join("sub")).expect("mkdir");
        fs::create_dir_all(&outside).expect("mkdir");
        fs::write(root.join("a.txt"), "hello\nworld\n").expect("write");
        fs::write(root.join("sub").join("b.md"), "# Notes\nfind me here\n").expect("write");
        fs::write(outside.join("key.txt"), "top secret").expect("write");
        (tmp, root, outside)
    }

    fn run(s: &Sandbox, op: FileOp, v: Value) -> Result<String, String> {
        s.run(op, &v)
    }

    #[test]
    fn paths_resolve_inside_the_grant() {
        let (_t, root, _) = setup();
        let s = sandbox(&[&root], false);
        let real = fs::canonicalize(&root).expect("real");
        assert_eq!(s.resolve("a.txt").expect("rel"), real.join("a.txt"));
        assert_eq!(
            s.resolve("work/a.txt").expect("by name"),
            real.join("a.txt")
        );
        assert_eq!(
            s.resolve(&root.join("sub/../a.txt").to_string_lossy())
                .expect("dots inside"),
            real.join("a.txt")
        );
        assert_eq!(
            s.resolve("new/deeper/c.txt").expect("new"),
            real.join("new/deeper/c.txt")
        );
        assert_eq!(s.resolve("").expect("root"), real);
    }

    #[test]
    fn escapes_are_refused() {
        let (_t, root, outside) = setup();
        let s = sandbox(&[&root], false);
        for bad in [
            "../secret/key.txt".to_owned(),
            "sub/../../secret/key.txt".to_owned(),
            outside.join("key.txt").to_string_lossy().into_owned(),
            root.join("..")
                .join("secret")
                .to_string_lossy()
                .into_owned(),
            "new/../../secret/x.txt".to_owned(),
            "missing/../../../x".to_owned(),
        ] {
            let err = s.resolve(&bad).expect_err(&bad);
            assert!(
                err.contains("outside") || err.contains(".."),
                "{bad}: {err}"
            );
        }
        // Nothing outside got read or written.
        assert!(run(&s, FileOp::Read, json!({"path": "../secret/key.txt"})).is_err());
        assert!(
            run(
                &s,
                FileOp::Write,
                json!({"path": "../secret/x.txt", "content": "x"})
            )
            .is_err()
        );
        assert!(!outside.join("x.txt").exists());
        assert!(
            run(
                &s,
                FileOp::Move,
                json!({"from": "a.txt", "to": "../secret/a.txt"})
            )
            .is_err()
        );
        assert!(root.join("a.txt").exists());
        // Prefix tricks: "work-evil" is not inside "work".
        let evil = root.with_file_name("work-evil");
        fs::create_dir_all(&evil).expect("mkdir");
        assert!(s.resolve(&evil.join("x").to_string_lossy()).is_err());
    }

    /// Thousands of generated paths (dots, separators, absolute paths, names of the grant and
    /// of a sibling, links, odd characters): whatever resolves lies inside the grant, and no
    /// write, move or delete through them touches anything outside (Phase 18 fuzzing).
    #[test]
    fn generated_paths_never_leave_the_grant() {
        let (_t, root, outside) = setup();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, root.join("link")).expect("link");
        let s = sandbox(&[&root], true);
        let real_root = fs::canonicalize(&root).expect("real");
        let real_outside = fs::canonicalize(&outside).expect("real");
        let parent = root
            .parent()
            .expect("parent")
            .to_string_lossy()
            .into_owned();
        let parts: Vec<String> = vec![
            "..".into(),
            ".".into(),
            "sub".into(),
            "a.txt".into(),
            "work".into(),
            "secret".into(),
            "key.txt".into(),
            "link".into(),
            "new".into(),
            "".into(),
            "...".into(),
            "~".into(),
            "%2e%2e".into(),
            "work-evil".into(),
            "æ ø".into(),
            "a\\..\\..".into(),
            "con".into(),
            " ".into(),
            root.to_string_lossy().into_owned(),
            outside.to_string_lossy().into_owned(),
            parent,
        ];
        // A small deterministic generator (no extra dependency).
        let mut seed: u64 = 0x9e37_79b9_7f4a_7c15;
        let mut next = |n: usize| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % n as u64) as usize
        };
        for _ in 0..3_000 {
            let count = 1 + next(5);
            let mut path = String::new();
            for k in 0..count {
                if k > 0 {
                    path.push(if next(4) == 0 { '\\' } else { '/' });
                }
                path.push_str(&parts[next(parts.len())]);
            }
            if let Ok(p) = s.resolve(&path) {
                assert!(p.starts_with(&real_root), "{path} → {}", p.display());
                assert!(!p.starts_with(&real_outside), "{path} → {}", p.display());
                assert!(
                    !p.components().any(|c| c == Component::ParentDir),
                    "{path} → {}",
                    p.display()
                );
            }
            // Writes and deletes through generated paths stay inside too.
            match next(3) {
                0 => {
                    let _ = s.run(FileOp::Write, &json!({"path": path, "content": "x"}));
                }
                1 => {
                    let _ = s.run(FileOp::Move, &json!({"from": "new", "to": path}));
                }
                _ => {
                    let _ = s.run(FileOp::Delete, &json!({"path": path}));
                }
            }
        }
        // The folder outside is exactly as it was.
        let names: Vec<String> = fs::read_dir(&outside)
            .expect("outside")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["key.txt".to_owned()]);
        assert_eq!(
            fs::read_to_string(outside.join("key.txt")).expect("key"),
            "top secret"
        );
        assert!(root.exists(), "the grant itself is never removed");
    }

    #[cfg(unix)]
    #[test]
    fn links_out_of_the_grant_are_refused() {
        let (_t, root, outside) = setup();
        std::os::unix::fs::symlink(&outside, root.join("link")).expect("link");
        std::os::unix::fs::symlink(outside.join("key.txt"), root.join("key-link.txt"))
            .expect("link");
        let s = sandbox(&[&root], false);
        assert!(s.resolve("link/key.txt").is_err());
        assert!(s.resolve("key-link.txt").is_err());
        assert!(s.resolve("link/new.txt").is_err());
        assert!(run(&s, FileOp::Read, json!({"path": "key-link.txt"})).is_err());
        // Listing shows the link but doesn't walk into it.
        let listing = run(&s, FileOp::List, json!({"depth": 3})).expect("lists");
        assert!(!listing.contains("key.txt  ("), "{listing}");
        let found = run(&s, FileOp::Search, json!({"text": "secret"})).expect("search");
        assert!(found.starts_with("Nothing found"), "{found}");
    }

    #[test]
    fn no_folders_means_no_paths() {
        let s = Sandbox::default();
        assert!(s.is_empty());
        assert!(
            s.resolve("a")
                .expect_err("no folders")
                .contains("No folders")
        );
        let missing = Sandbox::from_folders(
            &[ChatFolder {
                path: "/definitely/not/here".into(),
                name: "here".into(),
                added_at: 0,
            }],
            false,
        );
        assert!(missing.is_empty());
        assert!(resolve_grant("relative/path").is_err());
        assert!(resolve_grant("").is_err());
    }

    #[test]
    fn list_read_and_search() {
        let (_t, root, _) = setup();
        let s = sandbox(&[&root], false);
        let l = run(&s, FileOp::List, json!({})).expect("list");
        assert!(l.contains("sub/\na.txt  (12 B)"), "{l}");
        let l2 = run(&s, FileOp::List, json!({"path": "work", "depth": 2})).expect("list");
        assert!(l2.contains("  b.md"), "{l2}");
        let r = run(&s, FileOp::Read, json!({"path": "a.txt"})).expect("read");
        assert!(r.ends_with("hello\nworld\n"), "{r}");
        let r2 = run(&s, FileOp::Read, json!({"path": "a.txt", "start_line": 2})).expect("read");
        assert!(
            r2.contains("lines 2–2 of 2") && r2.ends_with("world\n"),
            "{r2}"
        );
        let part = run(&s, FileOp::Read, json!({"path": "a.txt", "max_lines": 1})).expect("read");
        assert!(part.contains("start_line 2"), "{part}");
        assert!(run(&s, FileOp::Read, json!({"path": "sub"})).is_err());
        fs::write(root.join("bin.dat"), [0u8, 159, 146, 150]).expect("bin");
        assert!(
            run(&s, FileOp::Read, json!({"path": "bin.dat"}))
                .expect_err("binary")
                .contains("isn't a text file")
        );
        let f = run(&s, FileOp::Search, json!({"text": "FIND ME"})).expect("search");
        assert!(f.contains("b.md:2: find me here"), "{f}");
        let f2 = run(&s, FileOp::Search, json!({"name": "*.md"})).expect("search");
        assert!(f2.starts_with("1 match") && f2.contains("b.md"), "{f2}");
        assert!(run(&s, FileOp::Search, json!({})).is_err());
    }

    #[test]
    fn write_edit_and_move() {
        let (_t, root, _) = setup();
        let s = sandbox(&[&root], false);
        let w = run(
            &s,
            FileOp::Write,
            json!({"path": "out/summary.md", "content": "Hi"}),
        )
        .expect("write");
        assert!(w.starts_with("Created"), "{w}");
        assert_eq!(
            fs::read_to_string(root.join("out/summary.md")).expect("read"),
            "Hi"
        );
        let w2 = run(
            &s,
            FileOp::Write,
            json!({"path": "out/summary.md", "content": "Hej"}),
        )
        .expect("write");
        assert!(w2.starts_with("Replaced"));
        run(
            &s,
            FileOp::Edit,
            json!({"path": "a.txt", "old_str": "world", "new_str": "there"}),
        )
        .expect("edit");
        assert_eq!(
            fs::read_to_string(root.join("a.txt")).expect("read"),
            "hello\nthere\n"
        );
        assert!(
            run(
                &s,
                FileOp::Edit,
                json!({"path": "a.txt", "old_str": "nope", "new_str": "x"})
            )
            .is_err()
        );
        // Into a new folder, then into an existing folder (keeps the name).
        run(
            &s,
            FileOp::Move,
            json!({"from": "a.txt", "to": "Text/a.txt"}),
        )
        .expect("move");
        assert!(root.join("Text/a.txt").exists() && !root.join("a.txt").exists());
        run(&s, FileOp::Move, json!({"from": "sub/b.md", "to": "Text"})).expect("move");
        assert!(root.join("Text/b.md").exists());
        // Never overwrites, and never moves a folder into itself or the grant itself.
        fs::write(root.join("c.txt"), "c").expect("write");
        assert!(
            run(
                &s,
                FileOp::Move,
                json!({"from": "c.txt", "to": "Text/a.txt"})
            )
            .is_err()
        );
        assert!(
            run(
                &s,
                FileOp::Move,
                json!({"from": "Text", "to": "Text/inner"})
            )
            .is_err()
        );
        let whole = root.to_string_lossy().into_owned();
        assert!(run(&s, FileOp::Move, json!({"from": whole, "to": "x"})).is_err());
        assert!(run(&s, FileOp::Write, json!({"path": "", "content": "x"})).is_err());
    }

    #[test]
    fn delete_moves_to_to_delete_unless_allowed() {
        let (_t, root, _) = setup();
        let s = sandbox(&[&root], false);
        let d = run(&s, FileOp::Delete, json!({"path": "a.txt"})).expect("delete");
        assert!(d.contains(TO_DELETE_DIR), "{d}");
        assert!(!root.join("a.txt").exists());
        assert!(root.join(TO_DELETE_DIR).join("a.txt").exists());
        // A second file of the same name gets a new name in the bin.
        fs::write(root.join("a.txt"), "again").expect("write");
        run(&s, FileOp::Delete, json!({"path": "a.txt"})).expect("delete");
        assert!(root.join(TO_DELETE_DIR).join("a (2).txt").exists());
        run(&s, FileOp::Delete, json!({"path": "sub/b.md"})).expect("delete");
        assert!(root.join(TO_DELETE_DIR).join("sub/b.md").exists());
        assert!(run(&s, FileOp::Delete, json!({"path": "_to_delete/a.txt"})).is_err());
        assert!(
            run(&s, FileOp::Delete, json!({"path": ""})).is_err(),
            "not the grant itself"
        );

        let s = sandbox(&[&root], true);
        run(&s, FileOp::Delete, json!({"path": TO_DELETE_DIR})).expect("delete");
        assert!(!root.join(TO_DELETE_DIR).exists());
    }

    #[test]
    fn several_folders() {
        let (t, root, _) = setup();
        let other = t.path().join("Photos");
        fs::create_dir_all(&other).expect("mkdir");
        fs::write(other.join("p.jpg"), "jpg").expect("write");
        let s = sandbox(&[&root, &other], false);
        assert_eq!(s.root_paths().len(), 2);
        assert!(
            s.resolve("Photos/p.jpg")
                .expect("by name")
                .ends_with("p.jpg")
        );
        // Moving between shared folders works.
        run(
            &s,
            FileOp::Move,
            json!({"from": "Photos/p.jpg", "to": "sub"}),
        )
        .expect("move");
        assert!(root.join("sub/p.jpg").exists());
    }

    #[test]
    fn wildcards_and_sizes() {
        assert!(wildcard("*.pdf", "report.pdf"));
        assert!(!wildcard("*.pdf", "report.pdf.txt"));
        assert!(wildcard("img_??.png", "img_01.png"));
        assert!(wildcard("*", ""));
        assert_eq!(human_size(1536), "1.5 KB");
        assert_eq!(display_path(Path::new(r"\\?\C:\a")), r"C:\a");
        assert_eq!(
            display_path(Path::new(r"\\?\UNC\srv\share")),
            r"\\srv\share"
        );
    }

    #[test]
    fn labels_for_the_log() {
        assert_eq!(
            file_label(FileOp::Move, &json!({"from": "a", "to": "b"})).as_deref(),
            Some("a → b")
        );
        assert_eq!(file_label(FileOp::List, &json!({})).as_deref(), Some("."));
        assert_eq!(
            file_label(FileOp::Search, &json!({"name": "*.md"})).as_deref(),
            Some("*.md")
        );
    }
}
