//! Settings → Data: "Export all" (a zip of every table as JSON plus attachments, skills,
//! extension manifests and the MCP config) and "Delete all" (every chat, setting, file and
//! keychain entry Oxide made). Phase 17.
//!
//! Secrets never leave the keychain in an export: API keys, sign-ins, pairing tokens and
//! sensitive extension settings live there, and the MCP config's `env` and `headers` values
//! are replaced before it goes into the zip.

use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use oxide_core::{DataDeleted, DataExport, SearchKeyKind};
use oxide_mcp::archive::{MAX_ZIP_ENTRY, ZipWriter};
use oxide_store::{SecretStore, Store, StoreError, delete_large};
use serde_json::{Value, json};

/// Errors from exporting or deleting data.
#[derive(Debug, thiserror::Error)]
pub enum DataError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("{0}")]
    Io(String),
    #[error("{0}")]
    Invalid(String),
}

fn io_err(what: &str) -> impl Fn(std::io::Error) -> DataError + '_ {
    move |e| DataError::Io(format!("{what}: {e}"))
}

/// Where Oxide keeps its files (the app's data, config and log folders).
#[derive(Debug, Clone)]
pub struct DataDirs {
    pub data: PathBuf,
    pub config: PathBuf,
    pub logs: PathBuf,
}

/// Written into the data folder when "Delete all" couldn't remove everything; the next start
/// removes what it lists before anything else opens.
pub const PENDING_DELETE_FILE: &str = ".oxide-delete-pending.json";

/// Folders of the data folder copied whole into an export.
const EXPORTED_FOLDERS: &[&str] = &["attachments", "skills"];
/// Value put in place of secrets in the exported MCP config.
const REMOVED: &str = "(removed from the export)";

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// Writes everything Oxide keeps (except secrets and caches) into the zip at `dest`.
/// The zip is written next to `dest` first and moved there when complete.
pub async fn export_all(
    store: &Store,
    dirs: &DataDirs,
    dest: &Path,
) -> Result<DataExport, DataError> {
    if dest.as_os_str().is_empty() {
        return Err(DataError::Invalid(
            "Choose where to save the export.".into(),
        ));
    }
    let tables = store.dump_tables().await?;
    let dirs = dirs.clone();
    let dest = dest.to_path_buf();
    tokio::task::spawn_blocking(move || write_export(&tables, &dirs, &dest))
        .await
        .map_err(|e| DataError::Io(format!("the export stopped: {e}")))?
}

fn write_export(
    tables: &[oxide_store::TableDump],
    dirs: &DataDirs,
    dest: &Path,
) -> Result<DataExport, DataError> {
    let mut partial = dest.as_os_str().to_owned();
    partial.push(".partial");
    let partial = PathBuf::from(partial);
    let file = std::fs::File::create(&partial).map_err(io_err("couldn't create the export"))?;
    let result = (|| {
        let now = now_ms();
        let mut zip = ZipWriter::new(BufWriter::new(file), now / 1000);
        let mut files = 0u32;
        let mut skipped = Vec::new();
        let mut rows = 0u64;
        let mut table_list = Vec::new();
        for t in tables {
            let body = serde_json::to_vec_pretty(&t.rows)
                .map_err(|e| DataError::Io(format!("couldn't write {}: {e}", t.name)))?;
            zip.add(&format!("database/{}.json", t.name), &body)
                .map_err(io_err("couldn't write the export"))?;
            rows += t.rows.len() as u64;
            table_list.push(json!({ "name": t.name, "rows": t.rows.len() }));
        }
        for folder in EXPORTED_FOLDERS {
            add_folder(
                &mut zip,
                &dirs.data.join(folder),
                folder,
                &mut files,
                &mut skipped,
            )?;
        }
        // Extensions: their manifests (the unpacked servers can be rebuilt from the bundles).
        let ext_root = dirs.data.join("extensions");
        if let Ok(entries) = std::fs::read_dir(&ext_root) {
            let mut names: Vec<_> = entries.flatten().map(|e| e.file_name()).collect();
            names.sort();
            for name in names {
                let manifest = ext_root.join(&name).join("manifest.json");
                if manifest.is_file() {
                    let entry = format!("extensions/{}/manifest.json", name.to_string_lossy());
                    add_file(&mut zip, &manifest, &entry, &mut files, &mut skipped)?;
                }
            }
        }
        let chrome = dirs.data.join("chrome.json");
        if chrome.is_file() {
            add_file(
                &mut zip,
                &chrome,
                "chrome-pairings.json",
                &mut files,
                &mut skipped,
            )?;
        }
        let config = dirs.config.join(oxide_mcp::config::CONFIG_FILE_NAME);
        if config.is_file() {
            match std::fs::read(&config)
                .ok()
                .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            {
                Some(mut v) => {
                    redact_mcp_config(&mut v);
                    let body = serde_json::to_vec_pretty(&v).unwrap_or_default();
                    zip.add(
                        &format!("config/{}", oxide_mcp::config::CONFIG_FILE_NAME),
                        &body,
                    )
                    .map_err(io_err("couldn't write the export"))?;
                    files += 1;
                }
                None => skipped.push(format!(
                    "{}: not valid JSON, so its secrets couldn't be removed",
                    oxide_mcp::config::CONFIG_FILE_NAME
                )),
            }
        }
        let manifest = json!({
            "app": oxide_core::APP_NAME,
            "version": oxide_core::version(),
            "format": 1,
            "exportedAt": now,
            "tables": table_list,
            "files": files,
            "skipped": skipped,
            "notIncluded": [
                "API keys, MCP sign-ins, Chrome pairing tokens and sensitive extension settings (they stay in the system keychain)",
                "env and headers values in the MCP config",
                "the managed Python environment, sandbox workspaces and browser profiles (caches that are rebuilt)",
                "installed extensions' server files (only their manifests)"
            ],
        });
        zip.add(
            "manifest.json",
            &serde_json::to_vec_pretty(&manifest).unwrap_or_default(),
        )
        .map_err(io_err("couldn't write the export"))?;
        zip.add("README.txt", README.as_bytes())
            .map_err(io_err("couldn't write the export"))?;
        zip.finish().map_err(io_err("couldn't finish the export"))?;
        Ok((tables.len(), rows, files, skipped))
    })();
    match result {
        Ok((tables, rows, files, skipped)) => {
            if dest.exists() {
                std::fs::remove_file(dest).map_err(io_err("couldn't replace the old file"))?;
            }
            std::fs::rename(&partial, dest).map_err(io_err("couldn't save the export"))?;
            let bytes = std::fs::metadata(dest).map(|m| m.len()).unwrap_or(0);
            Ok(DataExport {
                path: dest.to_string_lossy().into_owned(),
                tables: u32::try_from(tables).unwrap_or(u32::MAX),
                rows,
                files,
                bytes,
                skipped,
            })
        }
        Err(e) => {
            let _ = std::fs::remove_file(&partial);
            Err(e)
        }
    }
}

const README: &str = "Oxide data export\r
=================\r
\r
manifest.json      what this export holds and what it leaves out\r
database/*.json    every table of Oxide's database, one JSON array of rows per table\r
                   (chats and messages, projects and their knowledge, memory, artifacts,\r
                   settings, providers without their keys, scheduled tasks and runs, ...)\r
attachments/       files sent in chats\r
skills/            your skills, as folders with SKILL.md\r
extensions/        the manifest of each installed extension\r
config/            the MCP config, with env and headers values removed\r
chrome-pairings.json  paired browsers (no tokens)\r
\r
API keys and other secrets are not in this export: they stay in the system keychain.\r
";

/// Adds every regular file under `root` as `prefix/<relative path>`. Links are skipped.
fn add_folder<W: std::io::Write>(
    zip: &mut ZipWriter<W>,
    root: &Path,
    prefix: &str,
    files: &mut u32,
    skipped: &mut Vec<String>,
) -> Result<(), DataError> {
    let mut stack = vec![root.to_path_buf()];
    let mut found: Vec<PathBuf> = Vec::new();
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                stack.push(entry.path());
            } else if kind.is_file() {
                found.push(entry.path());
            }
        }
    }
    found.sort();
    for path in found {
        let rel = path
            .strip_prefix(root)
            .map(|r| {
                r.components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/")
            })
            .unwrap_or_default();
        add_file(zip, &path, &format!("{prefix}/{rel}"), files, skipped)?;
    }
    Ok(())
}

fn add_file<W: std::io::Write>(
    zip: &mut ZipWriter<W>,
    path: &Path,
    entry: &str,
    files: &mut u32,
    skipped: &mut Vec<String>,
) -> Result<(), DataError> {
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    if size > MAX_ZIP_ENTRY {
        skipped.push(format!("{entry}: larger than 4 GB"));
        return Ok(());
    }
    match std::fs::read(path) {
        Ok(data) => {
            zip.add(entry, &data)
                .map_err(io_err("couldn't write the export"))?;
            *files += 1;
        }
        Err(e) => skipped.push(format!("{entry}: {e}")),
    }
    Ok(())
}

/// Replaces the values of `env` and `headers` in every server of an MCP config (Oxide's own
/// or Claude Desktop's shape), so the export carries no keys.
pub fn redact_mcp_config(config: &mut Value) {
    fn redact_server(server: &mut Value) {
        for field in ["env", "headers"] {
            if let Some(map) = server.get_mut(field).and_then(Value::as_object_mut) {
                for v in map.values_mut() {
                    *v = Value::String(REMOVED.into());
                }
            }
        }
    }
    for key in ["mcpServers", "servers"] {
        if let Some(servers) = config.get_mut(key).and_then(Value::as_object_mut) {
            for server in servers.values_mut() {
                redact_server(server);
            }
        }
    }
}

/// Bytes used under `dir` (links not followed). Slow for big folders: run it off the async
/// runtime.
pub fn folder_size(dir: &Path) -> u64 {
    let mut total = 0u64;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in entries.flatten() {
            match e.file_type() {
                Ok(t) if t.is_dir() => stack.push(e.path()),
                Ok(t) if t.is_file() => total += e.metadata().map(|m| m.len()).unwrap_or(0),
                _ => {}
            }
        }
    }
    total
}

/// The keychain entries Oxide may have made, given the providers' ids and the MCP servers'
/// names (extension secrets and Chrome tokens go with their own uninstall/forget).
pub fn secret_names(provider_ids: &[String], mcp_servers: &[String]) -> Vec<String> {
    let mut names: Vec<String> = provider_ids
        .iter()
        .map(|id| oxide_store::provider_secret_name(id))
        .collect();
    names.extend(
        [SearchKeyKind::Brave, SearchKeyKind::Tavily]
            .iter()
            .map(|k| k.secret_name().to_owned()),
    );
    names.extend(mcp_servers.iter().map(|s| oxide_mcp::oauth::token_key(s)));
    // Settings → Media generation's own keys (Phase 19).
    names.extend(
        oxide_core::GenerationKind::ALL
            .iter()
            .map(|k| k.secret_name()),
    );
    // The first Phase 19 build also had a Text endpoint with a key of its own.
    names.push("generation:text".to_owned());
    names.sort();
    names.dedup();
    names
}

/// Removes the keychain entries named (long, split ones included). Returns how many were
/// removed without an error.
pub async fn delete_secrets(secrets: &Arc<dyn SecretStore>, names: Vec<String>) -> u32 {
    let secrets = secrets.clone();
    tokio::task::spawn_blocking(move || {
        let mut done = 0u32;
        for name in names {
            match delete_large(secrets.as_ref(), &name) {
                Ok(()) => done += 1,
                Err(e) => tracing::warn!("couldn't remove a keychain entry: {e}"),
            }
        }
        done
    })
    .await
    .unwrap_or(0)
}

/// The folders "Delete all" empties: the data folder, and the config and log folders when
/// they aren't inside it.
fn roots(dirs: &DataDirs) -> Vec<PathBuf> {
    let mut out = vec![dirs.data.clone()];
    for extra in [&dirs.config, &dirs.logs] {
        if !out
            .iter()
            .any(|r| extra.starts_with(r) || r.starts_with(extra))
        {
            out.push(extra.clone());
        }
    }
    out
}

/// Removes everything inside Oxide's folders (they stay, empty). What couldn't be removed
/// (a file another program holds open) is listed in [`PENDING_DELETE_FILE`] and returned.
/// Run with nothing open: the database must be closed first.
pub fn wipe_folders(dirs: &DataDirs) -> Vec<String> {
    let mut left: Vec<PathBuf> = Vec::new();
    for root in roots(dirs) {
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        for e in entries.flatten() {
            let path = e.path();
            if path.file_name().and_then(|n| n.to_str()) == Some(PENDING_DELETE_FILE) {
                continue;
            }
            if let Err(err) = remove_path(&path) {
                tracing::warn!("couldn't remove {}: {err}", path.display());
                left.push(path);
            }
        }
    }
    let marker = dirs.data.join(PENDING_DELETE_FILE);
    if left.is_empty() {
        let _ = std::fs::remove_file(&marker);
    } else {
        let list: Vec<String> = left
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        let _ = std::fs::create_dir_all(&dirs.data);
        if let Err(e) = std::fs::write(&marker, serde_json::to_vec(&list).unwrap_or_default()) {
            tracing::warn!("couldn't note what's left to delete: {e}");
        }
    }
    left.into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect()
}

fn remove_path(path: &Path) -> std::io::Result<()> {
    let meta = std::fs::symlink_metadata(path)?;
    if meta.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    }
}

/// At start, before anything opens: removes what an earlier "Delete all" couldn't. Only
/// paths inside Oxide's own folders are touched. Returns whether there was anything to do.
pub fn finish_pending_delete(dirs: &DataDirs) -> bool {
    let marker = dirs.data.join(PENDING_DELETE_FILE);
    let Ok(bytes) = std::fs::read(&marker) else {
        return false;
    };
    let list: Vec<String> = serde_json::from_slice(&bytes).unwrap_or_default();
    let roots = roots(dirs);
    let mut left = Vec::new();
    for p in list {
        let path = PathBuf::from(&p);
        let inside = roots.iter().any(|r| path.starts_with(r) && path != *r)
            && !path
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir));
        if !inside || !path.exists() {
            continue;
        }
        if let Err(e) = remove_path(&path) {
            tracing::warn!("still couldn't remove {}: {e}", path.display());
            left.push(p);
        }
    }
    if left.is_empty() {
        let _ = std::fs::remove_file(&marker);
    } else {
        let _ = std::fs::write(&marker, serde_json::to_vec(&left).unwrap_or_default());
    }
    tracing::info!("finished deleting data left by an earlier \"Delete all\"");
    true
}

/// What "Delete all" stops and removes. Each part is optional so the app can pass what it
/// has (computer use and the sandbox aren't on every system).
pub struct DeleteAll<'a> {
    pub store: &'a Store,
    pub secrets: &'a Arc<dyn SecretStore>,
    pub dirs: &'a DataDirs,
    pub chat: &'a crate::ChatService,
    pub mcp: &'a oxide_mcp::McpManager,
    pub extensions: &'a oxide_mcp::Extensions,
    pub chrome: &'a oxide_bridge::Bridge,
    pub sandboxes: &'a oxide_sandbox::Sandboxes,
    pub browser: &'a oxide_pyenv::BrowserAgents,
}

impl DeleteAll<'_> {
    /// Stops everything that runs, removes keychain entries, sandboxes and extensions,
    /// closes the database and empties Oxide's folders. The app must restart afterwards:
    /// nothing that was open works any more.
    pub async fn run(self) -> Result<DataDeleted, DataError> {
        for id in self.chat.running_chats() {
            self.chat.stop(&id);
        }
        let chats = self.store.all_conversation_ids().await?;
        let providers: Vec<String> = self
            .store
            .list_providers()
            .await?
            .into_iter()
            .map(|p| p.id)
            .collect();
        let mut servers: Vec<String> = self.mcp.states().into_iter().map(|s| s.name).collect();
        servers.extend(self.mcp.config_snapshot().names());

        // Sandboxes: containers and volumes go with their chats.
        for id in &chats {
            self.sandboxes.forget(id).await;
        }
        // Extensions take their sensitive settings with them.
        for ext in self.extensions.list().await {
            if let Err(e) = self.extensions.uninstall(&ext.name).await {
                tracing::warn!("couldn't uninstall an extension: {e}");
            }
        }
        // Chrome pairings: their tokens.
        let mut removed = 0u32;
        for browser in self.chrome.status().paired {
            self.chrome.forget(&browser.id).await;
            removed += 1;
        }
        self.chrome.shutdown().await;
        self.mcp.shutdown().await;
        let _ =
            tokio::time::timeout(std::time::Duration::from_secs(15), self.browser.shutdown()).await;
        removed += delete_secrets(self.secrets, secret_names(&providers, &servers)).await;

        self.store.close().await;
        let dirs = self.dirs.clone();
        let leftovers = tokio::task::spawn_blocking(move || wipe_folders(&dirs))
            .await
            .map_err(|e| DataError::Io(format!("deleting stopped: {e}")))?;
        Ok(DataDeleted {
            secrets: removed,
            leftovers,
        })
    }
}

#[cfg(test)]
mod tests {
    use oxide_store::{MemorySecrets, Secret, set_large};

    use super::*;

    fn dirs(root: &Path) -> DataDirs {
        DataDirs {
            data: root.join("data"),
            config: root.join("config"),
            logs: root.join("logs"),
        }
    }

    #[tokio::test]
    async fn exports_tables_files_and_a_redacted_config() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let d = dirs(tmp.path());
        std::fs::create_dir_all(d.data.join("attachments/ab")).expect("mkdir");
        std::fs::write(d.data.join("attachments/ab/photo.png"), b"\x89PNG data").expect("write");
        std::fs::create_dir_all(d.data.join("skills/haiku")).expect("mkdir");
        std::fs::write(
            d.data.join("skills/haiku/SKILL.md"),
            "---\nname: haiku\n---\nWrite one.",
        )
        .expect("write");
        std::fs::create_dir_all(d.data.join("extensions/files/server")).expect("mkdir");
        std::fs::write(
            d.data.join("extensions/files/manifest.json"),
            "{\"name\":\"files\"}",
        )
        .expect("write");
        std::fs::write(d.data.join("extensions/files/server/index.js"), "x").expect("write");
        std::fs::create_dir_all(d.data.join("python/venv")).expect("mkdir");
        std::fs::write(d.data.join("python/venv/big.bin"), vec![0u8; 1000]).expect("write");
        std::fs::create_dir_all(&d.config).expect("mkdir");
        std::fs::write(
            d.config.join(oxide_mcp::config::CONFIG_FILE_NAME),
            r#"{"mcpServers":{"gh":{"command":"gh-mcp","env":{"GITHUB_TOKEN":"ghp_secret"}},
                "remote":{"url":"https://x","headers":{"Authorization":"Bearer sk-1"}}}}"#,
        )
        .expect("write");

        let store = Store::open_in_memory().await.expect("store");
        store
            .add_memory(
                oxide_core::NewMemory {
                    project_id: None,
                    text: "Prefers metric units".into(),
                },
                None,
            )
            .await
            .expect("memory");

        let dest = tmp.path().join("out").join("oxide-export.zip");
        std::fs::create_dir_all(dest.parent().expect("parent")).expect("mkdir");
        let done = export_all(&store, &d, &dest).await.expect("exports");
        assert_eq!(done.path, dest.to_string_lossy());
        assert!(done.tables >= 10, "{done:?}");
        assert!(done.rows >= 1);
        assert_eq!(
            done.files, 4,
            "photo, SKILL.md, extension manifest, config: {done:?}"
        );
        assert!(done.skipped.is_empty());
        assert!(!tmp.path().join("out/oxide-export.zip.partial").exists());

        let mut f = std::fs::File::open(&dest).expect("open");
        let entries = oxide_mcp::archive::read_entries(&mut f).expect("zip");
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        for want in [
            "database/memories.json",
            "database/conversations.json",
            "attachments/ab/photo.png",
            "skills/haiku/SKILL.md",
            "extensions/files/manifest.json",
            "config/oxide_config.json",
            "manifest.json",
            "README.txt",
        ] {
            assert!(names.contains(&want), "{want} missing from {names:?}");
        }
        assert!(
            !names
                .iter()
                .any(|n| n.starts_with("python/") || n.contains("index.js"))
        );
        let read = |name: &str| {
            let e = entries.iter().find(|e| e.name == name).expect("entry");
            let mut f = std::fs::File::open(&dest).expect("open");
            String::from_utf8(oxide_mcp::archive::read_entry(&mut f, e, u64::MAX).expect("read"))
                .expect("utf-8")
        };
        assert!(read("database/memories.json").contains("Prefers metric units"));
        let config = read("config/oxide_config.json");
        assert!(
            !config.contains("ghp_secret") && !config.contains("sk-1"),
            "{config}"
        );
        assert!(config.contains("gh-mcp") && config.contains(REMOVED));
        let manifest: Value = serde_json::from_str(&read("manifest.json")).expect("json");
        assert_eq!(manifest["app"], "Oxide");
        assert_eq!(manifest["files"], 4);
    }

    #[tokio::test]
    async fn an_export_to_nowhere_is_refused_and_leaves_nothing() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = Store::open_in_memory().await.expect("store");
        let d = dirs(tmp.path());
        assert!(matches!(
            export_all(&store, &d, Path::new("")).await,
            Err(DataError::Invalid(_))
        ));
        let missing = tmp.path().join("no-such-folder").join("x.zip");
        assert!(matches!(
            export_all(&store, &d, &missing).await,
            Err(DataError::Io(_))
        ));
    }

    #[tokio::test]
    async fn secrets_of_providers_search_and_sign_ins_are_removed() {
        let secrets: Arc<dyn SecretStore> = Arc::new(MemorySecrets::default());
        let s = secrets.as_ref();
        s.set("provider:anthropic", &Secret::new("sk-ant"))
            .expect("set");
        s.set("websearch:brave", &Secret::new("brave"))
            .expect("set");
        s.set("generation:video", &Secret::new("sk-video"))
            .expect("set");
        set_large(s, "mcp-oauth:notion", &"t".repeat(5_000)).expect("set");
        s.set("unrelated", &Secret::new("keep")).expect("set");
        let names = secret_names(&["anthropic".into()], &["notion".into(), "notion".into()]);
        assert_eq!(names.iter().filter(|n| *n == "mcp-oauth:notion").count(), 1);
        delete_secrets(&secrets, names).await;
        for gone in [
            "provider:anthropic",
            "websearch:brave",
            "generation:video",
            "mcp-oauth:notion",
            "mcp-oauth:notion#1",
        ] {
            assert!(s.get(gone).expect("get").is_none(), "{gone} removed");
        }
        assert!(s.get("unrelated").expect("get").is_some());
    }

    #[test]
    fn wiping_empties_every_folder_and_retries_leftovers_at_start() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let d = dirs(tmp.path());
        for f in [
            "data/oxide.db",
            "data/attachments/a/x.png",
            "config/oxide_config.json",
            "logs/mcp-x.log",
        ] {
            let p = tmp.path().join(f);
            std::fs::create_dir_all(p.parent().expect("parent")).expect("mkdir");
            std::fs::write(p, b"x").expect("write");
        }
        let outside = tmp.path().join("elsewhere.txt");
        std::fs::write(&outside, b"keep").expect("write");
        assert!(wipe_folders(&d).is_empty());
        for root in [&d.data, &d.config, &d.logs] {
            assert!(root.is_dir(), "the folder stays");
            assert_eq!(
                std::fs::read_dir(root).expect("read").count(),
                0,
                "{} is empty",
                root.display()
            );
        }
        assert!(outside.exists());

        // A leftover from an earlier run is removed at start; paths outside are never touched.
        std::fs::create_dir_all(d.data.join("python")).expect("mkdir");
        std::fs::write(d.data.join("python/held.dll"), b"x").expect("write");
        std::fs::write(
            d.data.join(PENDING_DELETE_FILE),
            serde_json::to_vec(&[
                d.data.join("python").to_string_lossy().into_owned(),
                outside.to_string_lossy().into_owned(),
                d.data.to_string_lossy().into_owned(),
            ])
            .expect("json"),
        )
        .expect("write");
        assert!(finish_pending_delete(&d));
        assert!(!d.data.join("python").exists());
        assert!(outside.exists() && d.data.is_dir());
        assert!(!d.data.join(PENDING_DELETE_FILE).exists());
        assert!(!finish_pending_delete(&d), "nothing left to do");
    }

    #[test]
    fn config_inside_the_data_folder_is_wiped_once() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let d = DataDirs {
            data: tmp.path().join("app"),
            config: tmp.path().join("app"),
            logs: tmp.path().join("app").join("logs"),
        };
        assert_eq!(roots(&d), vec![tmp.path().join("app")]);
    }

    #[test]
    fn claude_desktop_shaped_configs_are_redacted_too() {
        let mut v = json!({"servers": {"a": {"env": {"K": "v"}}}, "other": {"env": {"K": "v"}}});
        redact_mcp_config(&mut v);
        assert_eq!(v["servers"]["a"]["env"]["K"], REMOVED);
        assert_eq!(v["other"]["env"]["K"], "v", "only servers are touched");
    }

    #[test]
    fn folder_sizes_add_up() {
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(tmp.path().join("a/b")).expect("mkdir");
        std::fs::write(tmp.path().join("a/one"), vec![0u8; 10]).expect("write");
        std::fs::write(tmp.path().join("a/b/two"), vec![0u8; 32]).expect("write");
        assert_eq!(folder_size(tmp.path()), 42);
    }

    struct NoEvents;
    impl crate::EventSink for NoEvents {
        fn stream_event(&self, _: oxide_core::ChatStreamEvent) {}
        fn conversation_changed(&self, _: oxide_core::Conversation) {}
    }

    struct NoMcp;
    impl oxide_mcp::ManagerSink for NoMcp {
        fn event(&self, _: oxide_core::McpEvent) {}
    }
    impl oxide_mcp::UserAgent for NoMcp {
        fn sample(
            &self,
            _: Value,
            _: Option<String>,
        ) -> futures_util::future::BoxFuture<'_, Result<Value, String>> {
            Box::pin(async { Err("no".into()) })
        }
        fn open_url(&self, _: &str) -> Result<(), String> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn delete_all_removes_chats_files_settings_and_keys() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let d = dirs(tmp.path());
        std::fs::create_dir_all(d.data.join("attachments")).expect("mkdir");
        std::fs::write(d.data.join("attachments/a.txt"), b"x").expect("write");
        std::fs::create_dir_all(&d.config).expect("mkdir");
        let config = d.config.join(oxide_mcp::config::CONFIG_FILE_NAME);
        std::fs::write(&config, r#"{"mcpServers":{}}"#).expect("write");
        std::fs::create_dir_all(&d.logs).expect("mkdir");
        std::fs::write(d.logs.join("mcp-x.log"), b"log").expect("write");

        let store = Store::open(&d.data.join(oxide_store::DB_FILE_NAME))
            .await
            .expect("store");
        let secrets: Arc<dyn SecretStore> = Arc::new(MemorySecrets::default());
        let provider = store
            .save_provider(oxide_core::ProviderInput {
                id: None,
                kind: oxide_core::ProviderKind::Anthropic,
                name: "Anthropic".into(),
                base_url: "https://api.anthropic.com".into(),
                enabled: true,
            })
            .await
            .expect("provider");
        let key = oxide_store::provider_secret_name(&provider.id);
        secrets.set(&key, &Secret::new("sk-ant")).expect("set");
        secrets
            .set("websearch:tavily", &Secret::new("tv"))
            .expect("set");
        store
            .start_conversation(oxide_core::StartConversation {
                text: "hello".into(),
                model: "p/m".into(),
                project_id: None,
                attachments: vec![],
                folders: vec![],
                tools: None,
            })
            .await
            .expect("chat");

        let registry = Arc::new(crate::ModelRegistry::new(store.clone(), secrets.clone()));
        let chat = crate::ChatService::new(
            store.clone(),
            registry,
            Arc::new(NoEvents),
            crate::Tools::new(None, secrets.clone(), None),
        );
        let tokens = Arc::new(oxide_mcp::oauth::MemoryTokens::default());
        let mcp = oxide_mcp::McpManager::new(
            config,
            Some(d.logs.clone()),
            Arc::new(NoMcp),
            Arc::new(NoMcp),
            tokens.clone(),
            oxide_mcp::ManagerOptions::default(),
        );
        let extensions = oxide_mcp::Extensions::new(d.data.join("extensions"), mcp.clone(), tokens);
        let chrome = oxide_bridge::Bridge::open(
            d.data.join("chrome.json"),
            secrets.clone(),
            Arc::new(oxide_bridge::NoSink),
        )
        .await;
        let sandboxes = oxide_sandbox::Sandboxes::new(
            d.data.join("sandboxes"),
            None,
            Arc::new(oxide_sandbox::NoSink),
        );
        let browser = oxide_pyenv::BrowserAgents::new(
            oxide_pyenv::PythonEnv::new(
                oxide_pyenv::PythonConfig::new(d.data.join("python")),
                Arc::new(oxide_pyenv::NoSink),
            ),
            Arc::new(oxide_pyenv::NoBrowserSink),
            None,
        );

        let done = DeleteAll {
            store: &store,
            secrets: &secrets,
            dirs: &d,
            chat: &chat,
            mcp: &mcp,
            extensions: &extensions,
            chrome: &chrome,
            sandboxes: &sandboxes,
            browser: &browser,
        }
        .run()
        .await
        .expect("deletes");
        assert!(done.leftovers.is_empty(), "{done:?}");
        assert!(secrets.get(&key).expect("get").is_none());
        assert!(secrets.get("websearch:tavily").expect("get").is_none());
        for root in [&d.data, &d.config, &d.logs] {
            assert_eq!(
                std::fs::read_dir(root).expect("read").count(),
                0,
                "{} is empty",
                root.display()
            );
        }
        // A fresh start opens an empty database.
        let fresh = Store::open(&d.data.join(oxide_store::DB_FILE_NAME))
            .await
            .expect("reopens");
        assert!(fresh.all_conversation_ids().await.expect("ids").is_empty());
        let providers = fresh.list_providers().await.expect("providers");
        assert!(
            !providers.iter().any(|p| p.id == provider.id),
            "only the defaults again"
        );
    }
}
