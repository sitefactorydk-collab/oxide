//! An app's own folder (`oxide.folder`): one folder on this computer that the user picked for
//! the app with Oxide's folder picker ([`AppPrefs::folder`]). The app reads and writes inside
//! it with relative, `/`-separated paths ([`valid_app_path`]).
//!
//! Paths are resolved like the chat's file tools resolve theirs (existing parts canonicalized,
//! so a symlink can't lead out), the folder itself can't be deleted or renamed, symlinks are
//! neither listed nor followed, and Oxide's own data, config and log folders stay out of reach
//! even when the picked folder contains them (an app writing Oxide's config could start
//! programs).

use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use base64::Engine as _;
use oxide_core::{AppCallError, AppPrefs, valid_app_path};
use serde_json::{Value, json};

use crate::tools::files::resolve_existing_prefix;

/// Most bytes one file read or written through the SDK may have.
pub const MAX_FOLDER_FILE_BYTES: u64 = 20 * 1024 * 1024;
/// Most entries one `folder.list` returns.
pub const MAX_FOLDER_ENTRIES: usize = 5_000;
/// How deep `folder.list` may walk.
const MAX_DEPTH: u64 = 12;

fn failed(msg: impl Into<String>) -> AppCallError {
    AppCallError::failed(msg)
}

fn str_param<'a>(p: &'a Value, key: &str) -> &'a str {
    p.get(key).and_then(Value::as_str).unwrap_or("")
}

/// The app's folder, resolved. It must still exist.
pub fn root(prefs: &AppPrefs) -> Result<PathBuf, AppCallError> {
    let Some(folder) = prefs.folder.as_deref() else {
        return Err(failed(
            "No folder is chosen for this app yet. Choose one with folder.choose() or in the \
             app's Settings.",
        ));
    };
    let real = fs::canonicalize(folder).map_err(|_| {
        failed(format!(
            "The app's folder “{folder}” can't be opened any more. Choose it again."
        ))
    })?;
    if !real.is_dir() {
        return Err(failed(format!(
            "The app's folder “{folder}” isn't a folder any more. Choose it again."
        )));
    }
    Ok(real)
}

/// What an app may reach: its folder, minus Oxide's own folders.
pub struct Scope {
    root: PathBuf,
    protected: Vec<PathBuf>,
}

impl Scope {
    /// `protected` are folders the app may never reach (Oxide's data, config and logs).
    pub fn new(root: PathBuf, protected: &[PathBuf]) -> Self {
        // Resolved where they exist (paths inside the folder are resolved too).
        let protected = protected
            .iter()
            .map(|p| fs::canonicalize(p).unwrap_or_else(|_| p.clone()))
            .collect();
        Self { root, protected }
    }

    fn is_protected(&self, p: &Path) -> bool {
        self.protected.iter().any(|x| p.starts_with(x))
    }

    /// A relative path inside the folder. `""` (or `"."`) is the folder itself when
    /// `allow_root`.
    pub fn resolve(&self, rel: &str, allow_root: bool) -> Result<PathBuf, AppCallError> {
        let rel = rel.trim();
        if rel.is_empty() || rel == "." {
            return if allow_root {
                Ok(self.root.clone())
            } else {
                Err(failed("A path inside the folder is needed."))
            };
        }
        if !valid_app_path(rel) {
            return Err(failed(format!(
                "“{rel}” isn't a path inside the app's folder: use a relative path with / \
                 (no .., no drive letters, no backslashes)."
            )));
        }
        let resolved = resolve_existing_prefix(&self.root.join(rel)).map_err(failed)?;
        if !resolved.starts_with(&self.root) || resolved == self.root {
            return Err(failed(format!("“{rel}” is outside the app's folder.")));
        }
        if self.is_protected(&resolved) {
            return Err(failed("Oxide's own folders can't be reached by apps."));
        }
        Ok(resolved)
    }

    /// The `/`-separated path of `p` relative to the folder.
    fn rel(&self, p: &Path) -> String {
        p.strip_prefix(&self.root)
            .map(|r| r.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default()
    }

    fn list(&self, path: &str, depth: u64) -> Result<Value, AppCallError> {
        let start = self.resolve(path, true)?;
        let meta = fs::symlink_metadata(&start)
            .map_err(|_| failed(format!("“{path}” doesn't exist in the app's folder.")))?;
        if !meta.is_dir() {
            return Err(failed(format!("“{path}” isn't a folder.")));
        }
        let mut entries = Vec::new();
        let mut truncated = false;
        self.walk(
            &start,
            1,
            depth.clamp(1, MAX_DEPTH),
            &mut entries,
            &mut truncated,
        );
        entries.sort_by(|a: &Value, b: &Value| str_param(a, "path").cmp(str_param(b, "path")));
        Ok(json!({ "entries": entries, "truncated": truncated }))
    }

    fn walk(&self, dir: &Path, level: u64, depth: u64, out: &mut Vec<Value>, cut: &mut bool) {
        let Ok(read) = fs::read_dir(dir) else {
            return;
        };
        for entry in read.flatten() {
            if out.len() >= MAX_FOLDER_ENTRIES {
                *cut = true;
                return;
            }
            let path = entry.path();
            let Ok(meta) = fs::symlink_metadata(&path) else {
                continue;
            };
            if meta.file_type().is_symlink() || self.is_protected(&path) {
                continue;
            }
            let rel = self.rel(&path);
            if !valid_app_path(&rel) {
                continue;
            }
            let modified = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64);
            if meta.is_dir() {
                out.push(json!({ "path": rel, "kind": "dir", "size": 0, "modified": modified }));
                if level < depth {
                    self.walk(&path, level + 1, depth, out, cut);
                }
            } else if meta.is_file() {
                out.push(json!({
                    "path": rel, "kind": "file", "size": meta.len(), "modified": modified,
                }));
            }
        }
    }

    fn read(&self, path: &str, base64: bool) -> Result<Value, AppCallError> {
        let p = self.resolve(path, false)?;
        let meta = fs::metadata(&p)
            .map_err(|_| failed(format!("“{path}” doesn't exist in the app's folder.")))?;
        if !meta.is_file() {
            return Err(failed(format!("“{path}” isn't a file.")));
        }
        if meta.len() > MAX_FOLDER_FILE_BYTES {
            return Err(failed(format!("“{path}” is larger than 20 MB.")));
        }
        let bytes = fs::read(&p).map_err(|e| failed(format!("“{path}” can't be read: {e}")))?;
        let content = if base64 {
            base64::engine::general_purpose::STANDARD.encode(&bytes)
        } else {
            String::from_utf8(bytes).map_err(|_| {
                failed(format!(
                    "“{path}” isn't a text file; read it with folder.readBytes."
                ))
            })?
        };
        Ok(json!({ "content": content, "size": meta.len() }))
    }

    fn write(&self, path: &str, bytes: &[u8]) -> Result<(), AppCallError> {
        if bytes.len() as u64 > MAX_FOLDER_FILE_BYTES {
            return Err(failed(format!("“{path}” would be larger than 20 MB.")));
        }
        let p = self.resolve(path, false)?;
        if p.is_dir() {
            return Err(failed(format!("“{path}” is a folder.")));
        }
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| failed(format!("The folder for “{path}” can't be made: {e}")))?;
        }
        fs::write(&p, bytes).map_err(|e| failed(format!("“{path}” can't be written: {e}")))
    }

    fn mkdir(&self, path: &str) -> Result<(), AppCallError> {
        let p = self.resolve(path, false)?;
        fs::create_dir_all(&p).map_err(|e| failed(format!("“{path}” can't be made: {e}")))
    }

    fn delete(&self, path: &str) -> Result<(), AppCallError> {
        let p = self.resolve(path, false)?;
        let meta = fs::symlink_metadata(&p)
            .map_err(|_| failed(format!("“{path}” doesn't exist in the app's folder.")))?;
        let r = if meta.is_dir() {
            fs::remove_dir_all(&p)
        } else {
            fs::remove_file(&p)
        };
        r.map_err(|e| failed(format!("“{path}” can't be deleted: {e}")))
    }

    fn rename(&self, from: &str, to: &str) -> Result<(), AppCallError> {
        let a = self.resolve(from, false)?;
        let b = self.resolve(to, false)?;
        if fs::symlink_metadata(&a).is_err() {
            return Err(failed(format!(
                "“{from}” doesn't exist in the app's folder."
            )));
        }
        if fs::symlink_metadata(&b).is_ok() {
            return Err(failed(format!("“{to}” already exists.")));
        }
        if b.starts_with(&a) {
            return Err(failed(format!("“{from}” can't be moved into itself.")));
        }
        if let Some(parent) = b.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| failed(format!("The folder for “{to}” can't be made: {e}")))?;
        }
        fs::rename(&a, &b).map_err(|e| failed(format!("“{from}” can't be renamed: {e}")))
    }

    /// Runs one `folder.*` method (except `folder.get`, which needs no folder).
    pub fn run(&self, method: &str, p: &Value) -> Result<Value, AppCallError> {
        match method {
            "folder.list" => {
                let depth = p.get("depth").and_then(Value::as_u64).unwrap_or(1);
                self.list(str_param(p, "path"), depth)
            }
            "folder.read" => self.read(str_param(p, "path"), str_param(p, "as") == "base64"),
            "folder.write" => {
                let path = str_param(p, "path");
                let bytes = match (p.get("text"), p.get("base64")) {
                    (Some(Value::String(t)), _) => t.as_bytes().to_vec(),
                    (_, Some(Value::String(b))) => base64::engine::general_purpose::STANDARD
                        .decode(b.as_bytes())
                        .map_err(|_| failed("folder.write: the data isn't valid base64"))?,
                    _ => return Err(failed("folder.write needs text or base64 data")),
                };
                self.write(path, &bytes).map(|()| Value::Null)
            }
            "folder.mkdir" => self.mkdir(str_param(p, "path")).map(|()| Value::Null),
            "folder.delete" => self.delete(str_param(p, "path")).map(|()| Value::Null),
            "folder.rename" => self
                .rename(str_param(p, "from"), str_param(p, "to"))
                .map(|()| Value::Null),
            _ => Err(AppCallError::new(
                oxide_core::AppErrorCode::Unknown,
                format!("oxide has no method “{method}”"),
            )),
        }
    }
}

/// `folder.get`: the folder's path and name, or `null` when none is chosen.
pub fn info(prefs: &AppPrefs) -> Value {
    match prefs.folder.as_deref() {
        None => Value::Null,
        Some(f) => {
            let name = Path::new(f)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| f.to_owned());
            json!({ "path": f, "name": name, "exists": Path::new(f).is_dir() })
        }
    }
}

/// Answers one `folder.*` call (off the async runtime: it touches the disk).
pub async fn call(
    prefs: &AppPrefs,
    protected: &[PathBuf],
    method: &str,
    p: &Value,
) -> Result<Value, AppCallError> {
    if method == "folder.get" {
        return Ok(info(prefs));
    }
    let scope = Scope::new(root(prefs)?, protected);
    let method = method.to_owned();
    let p = p.clone();
    tokio::task::spawn_blocking(move || scope.run(&method, &p))
        .await
        .map_err(|e| failed(format!("the folder call failed: {e}")))?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope(dir: &Path) -> Scope {
        let root = fs::canonicalize(dir).expect("root");
        Scope::new(root.clone(), &[root.join("oxide-data")])
    }

    #[test]
    fn files_are_written_read_listed_renamed_and_deleted_inside_the_folder() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let s = scope(tmp.path());
        s.run(
            "folder.write",
            &json!({ "path": "plg_x/x.xml", "text": "<extension/>" }),
        )
        .expect("writes");
        s.run(
            "folder.write",
            &json!({ "path": "plg_x/icon.png", "base64": "iVBORw==" }),
        )
        .expect("writes bytes");
        let read = s
            .run("folder.read", &json!({ "path": "plg_x/x.xml" }))
            .expect("reads");
        assert_eq!(read["content"], "<extension/>");
        let bytes = s
            .run(
                "folder.read",
                &json!({ "path": "plg_x/icon.png", "as": "base64" }),
            )
            .expect("reads bytes");
        assert_eq!(bytes["content"], "iVBORw==");
        assert!(
            s.run("folder.read", &json!({ "path": "plg_x/icon.png" }))
                .is_err()
        );

        let top = s.run("folder.list", &json!({})).expect("lists");
        let paths: Vec<&str> = top["entries"]
            .as_array()
            .expect("entries")
            .iter()
            .map(|e| e["path"].as_str().unwrap_or(""))
            .collect();
        assert_eq!(paths, vec!["plg_x"]);
        let deep = s.run("folder.list", &json!({ "depth": 5 })).expect("lists");
        assert_eq!(deep["entries"].as_array().expect("entries").len(), 3);
        assert_eq!(deep["entries"][1]["path"], "plg_x/icon.png");
        assert_eq!(deep["entries"][1]["kind"], "file");
        assert_eq!(deep["entries"][1]["size"], 4);

        s.run("folder.rename", &json!({ "from": "plg_x", "to": "plg_y" }))
            .expect("renames");
        assert!(tmp.path().join("plg_y/x.xml").is_file());
        assert!(
            s.run(
                "folder.rename",
                &json!({ "from": "plg_y", "to": "plg_y/inner" })
            )
            .is_err()
        );
        s.run("folder.mkdir", &json!({ "path": "plg_y/addons/a" }))
            .expect("mkdir");
        s.run("folder.delete", &json!({ "path": "plg_y" }))
            .expect("deletes");
        assert!(!tmp.path().join("plg_y").exists());
        assert!(tmp.path().exists());
    }

    #[test]
    fn paths_cannot_leave_the_folder_or_reach_oxides_own() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let s = scope(tmp.path());
        fs::create_dir_all(tmp.path().join("oxide-data")).expect("dir");
        for bad in [
            "../x.txt",
            "a/../../x.txt",
            "/etc/passwd",
            "C:/Windows/x",
            "a\\b",
            "oxide-data/config.json",
        ] {
            assert!(
                s.run("folder.write", &json!({ "path": bad, "text": "x" }))
                    .is_err(),
                "{bad}"
            );
            assert!(
                s.run("folder.read", &json!({ "path": bad })).is_err(),
                "{bad}"
            );
        }
        // The folder itself can't be deleted or renamed.
        assert!(s.run("folder.delete", &json!({ "path": "" })).is_err());
        assert!(s.run("folder.delete", &json!({ "path": "." })).is_err());
        assert!(
            s.run("folder.rename", &json!({ "from": "", "to": "b" }))
                .is_err()
        );
        // Oxide's folder isn't listed.
        let list = s.run("folder.list", &json!({})).expect("lists");
        assert!(list["entries"].as_array().expect("entries").is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_not_followed_out_of_the_folder() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let outside = tempfile::tempdir().expect("outside");
        fs::write(outside.path().join("secret.txt"), "s").expect("write");
        std::os::unix::fs::symlink(outside.path(), tmp.path().join("link")).expect("symlink");
        let s = scope(tmp.path());
        assert!(
            s.run("folder.read", &json!({ "path": "link/secret.txt" }))
                .is_err()
        );
        assert!(
            s.run(
                "folder.write",
                &json!({ "path": "link/new.txt", "text": "x" })
            )
            .is_err()
        );
        let list = s.run("folder.list", &json!({})).expect("lists");
        assert!(list["entries"].as_array().expect("entries").is_empty());
    }

    #[test]
    fn info_names_the_folder_and_root_needs_one() {
        assert_eq!(info(&AppPrefs::default()), Value::Null);
        assert!(root(&AppPrefs::default()).is_err());
        let tmp = tempfile::tempdir().expect("tempdir");
        let prefs = AppPrefs {
            folder: Some(tmp.path().to_string_lossy().into_owned()),
            ..AppPrefs::default()
        };
        let v = info(&prefs);
        assert_eq!(v["exists"], true);
        assert!(root(&prefs).is_ok());
    }
}
