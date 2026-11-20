//! Utility Apps (Phase 20): the library of apps (built-in and the user's own), their
//! settings, grants and storage, export and import. [`runtime`] answers the apps' SDK calls.
//!
//! Built-in apps ship inside Oxide ([`builtin`]) and can't be changed; **Clone** copies one
//! into `<app data>/apps/<id>/`, where the user's own apps live. Settings, grants and
//! storage are in the database (keyed by app id), never in the folder, so an exported
//! `.oxideapp` carries only the app's files.

pub mod builtin;
pub mod crawl;
pub mod runtime;

use std::collections::HashMap;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use oxide_core::{
    APP_MANIFEST, AppBundle, AppCapability, AppFile, AppImportPreview, AppManifest, AppPrefs,
    GenerationKind, MAX_APP_BYTES, MAX_APP_FILE_BYTES, MAX_APP_FILES, UtilityApp, app_secret_name,
    app_slug, valid_app_path,
};
use oxide_store::{Secret, SecretStore, Store};

use crate::AgentError;

/// How long a read `.oxideapp` waits to be installed.
const IMPORT_TTL: Duration = Duration::from_secs(30 * 60);

fn invalid(msg: impl Into<String>) -> AgentError {
    AgentError::Invalid(msg.into())
}

/// The app library.
#[derive(Clone)]
pub struct UtilityApps {
    inner: Arc<Inner>,
}

struct Inner {
    /// `<app data>/apps`: one folder per own app.
    dir: PathBuf,
    store: Store,
    secrets: Arc<dyn SecretStore>,
    imports: Mutex<HashMap<String, PendingImport>>,
}

struct PendingImport {
    manifest: AppManifest,
    files: Vec<(String, Vec<u8>)>,
    at: Instant,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// An app's files read from a folder: `(path, bytes)`, sorted by path.
fn read_folder(root: &Path) -> Result<Vec<(String, Vec<u8>)>, String> {
    fn walk(
        root: &Path,
        dir: &Path,
        out: &mut Vec<(String, Vec<u8>)>,
        total: &mut u64,
    ) -> Result<(), String> {
        let entries = std::fs::read_dir(dir).map_err(|e| format!("can't read the folder: {e}"))?;
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            if meta.file_type().is_symlink() {
                continue;
            }
            let rel = path
                .strip_prefix(root)
                .map_err(|_| "a file is outside the app's folder".to_owned())?
                .to_string_lossy()
                .replace('\\', "/");
            if meta.is_dir() {
                walk(root, &path, out, total)?;
                continue;
            }
            if !valid_app_path(&rel) {
                continue;
            }
            if meta.len() > MAX_APP_FILE_BYTES {
                return Err(format!("{rel} is larger than 2 MB"));
            }
            *total += meta.len();
            if *total > MAX_APP_BYTES || out.len() >= MAX_APP_FILES {
                return Err("the app is larger than 20 MB or has more than 200 files".into());
            }
            let bytes = std::fs::read(&path).map_err(|e| format!("can't read {rel}: {e}"))?;
            out.push((rel, bytes));
        }
        Ok(())
    }
    let mut out = Vec::new();
    let mut total = 0;
    walk(root, root, &mut out, &mut total)?;
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

/// The manifest among an app's files, checked against the files: the entry must be there.
fn manifest_of(files: &[(String, Vec<u8>)]) -> Result<AppManifest, String> {
    let raw = files
        .iter()
        .find(|(p, _)| p == APP_MANIFEST)
        .map(|(_, b)| b)
        .ok_or_else(|| format!("{APP_MANIFEST} is missing"))?;
    let manifest: AppManifest =
        serde_json::from_slice(raw).map_err(|e| format!("{APP_MANIFEST} isn't valid: {e}"))?;
    manifest.validate()?;
    if !files.iter().any(|(p, _)| *p == manifest.entry) {
        return Err(format!("the entry file {} is missing", manifest.entry));
    }
    Ok(manifest)
}

/// Text files the frame loads (other files stay in the folder and in exports).
fn text_files(files: Vec<(String, Vec<u8>)>) -> Vec<AppFile> {
    files
        .into_iter()
        .filter_map(|(path, bytes)| {
            String::from_utf8(bytes)
                .ok()
                .map(|content| AppFile { path, content })
        })
        .collect()
}

impl UtilityApps {
    pub fn new(dir: impl Into<PathBuf>, store: Store, secrets: Arc<dyn SecretStore>) -> Self {
        Self {
            inner: Arc::new(Inner {
                dir: dir.into(),
                store,
                secrets,
                imports: Mutex::new(HashMap::new()),
            }),
        }
    }

    /// `<app data>/apps`.
    pub fn dir(&self) -> &Path {
        &self.inner.dir
    }

    pub fn store(&self) -> &Store {
        &self.inner.store
    }

    fn own_dir(&self, id: &str) -> Result<PathBuf, AgentError> {
        if !oxide_core::valid_app_id(id) {
            return Err(AgentError::NotFound("app"));
        }
        Ok(self.inner.dir.join(id))
    }

    fn is_builtin(id: &str) -> bool {
        builtin::find(id).is_some()
    }

    /// Every app: the built-in ones first, then the user's by name.
    pub async fn list(&self) -> Vec<UtilityApp> {
        let mut apps: Vec<UtilityApp> =
            builtin::all().iter().filter_map(|b| b.app().ok()).collect();
        let dir = self.inner.dir.clone();
        let own = tokio::task::spawn_blocking(move || {
            let mut out = Vec::new();
            let Ok(entries) = std::fs::read_dir(&dir) else {
                return out;
            };
            for e in entries.flatten() {
                let path = e.path();
                if !path.is_dir() {
                    continue;
                }
                let name = e.file_name().to_string_lossy().into_owned();
                if !oxide_core::valid_app_id(&name) || name.starts_with('.') {
                    continue;
                }
                out.push(own_app(&name, &path));
            }
            out
        })
        .await
        .unwrap_or_default();
        let mut own: Vec<UtilityApp> = own
            .into_iter()
            .filter(|a| !Self::is_builtin(&a.manifest.id))
            .collect();
        own.sort_by_key(|a| a.manifest.name.to_lowercase());
        apps.extend(own);
        apps
    }

    /// One app's files (own apps are read from disk each time, so edits show on reload).
    pub async fn bundle(&self, id: &str) -> Result<AppBundle, AgentError> {
        if let Some(b) = builtin::find(id) {
            let app = b.app().map_err(invalid)?;
            return Ok(AppBundle {
                app,
                files: b.text_files(),
            });
        }
        let dir = self.own_dir(id)?;
        let id = id.to_owned();
        tokio::task::spawn_blocking(move || {
            if !dir.is_dir() {
                return Err(AgentError::NotFound("app"));
            }
            let app = own_app(&id, &dir);
            if let Some(p) = &app.problem {
                return Err(invalid(format!("The app can't run: {p}.")));
            }
            let files = read_folder(&dir).map_err(invalid)?;
            Ok(AppBundle {
                app,
                files: text_files(files),
            })
        })
        .await
        .map_err(|e| invalid(format!("reading the app failed: {e}")))?
    }

    /// An app's manifest (for permission checks and defaults).
    pub async fn manifest(&self, id: &str) -> Result<AppManifest, AgentError> {
        if let Some(b) = builtin::find(id) {
            return b.app().map(|a| a.manifest).map_err(invalid);
        }
        let dir = self.own_dir(id)?;
        let id = id.to_owned();
        tokio::task::spawn_blocking(move || {
            if !dir.is_dir() {
                return Err(AgentError::NotFound("app"));
            }
            let app = own_app(&id, &dir);
            match app.problem {
                Some(p) => Err(invalid(format!("The app can't run: {p}."))),
                None => Ok(app.manifest),
            }
        })
        .await
        .map_err(|e| invalid(format!("reading the app failed: {e}")))?
    }

    /// An id no app uses yet, made from `name`.
    fn free_id(&self, name: &str) -> String {
        let base = app_slug(name);
        let taken = |id: &str| Self::is_builtin(id) || self.inner.dir.join(id).exists();
        if !taken(&base) {
            return base;
        }
        (2..)
            .map(|n| format!("{base}-{n}"))
            .find(|id| !taken(id))
            .unwrap_or(base)
    }

    /// Writes an own app's folder from files (replacing an older copy).
    async fn write_app(
        &self,
        id: &str,
        files: Vec<(String, Vec<u8>)>,
    ) -> Result<UtilityApp, AgentError> {
        let dir = self.own_dir(id)?;
        let root = self.inner.dir.clone();
        let id = id.to_owned();
        tokio::task::spawn_blocking(move || {
            std::fs::create_dir_all(&root).map_err(|e| invalid(format!("can't make {e}")))?;
            // Written next to the target first, so a failure leaves the old copy.
            let tmp = root.join(format!(".{id}.partial"));
            let _ = std::fs::remove_dir_all(&tmp);
            for (path, bytes) in &files {
                if !valid_app_path(path) {
                    return Err(invalid(format!("“{path}” isn't a valid file path")));
                }
                let target = tmp.join(path);
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| invalid(format!("can't write the app: {e}")))?;
                }
                std::fs::write(&target, bytes)
                    .map_err(|e| invalid(format!("can't write the app: {e}")))?;
            }
            if dir.exists() {
                let old = root.join(format!(".{id}.old"));
                let _ = std::fs::remove_dir_all(&old);
                std::fs::rename(&dir, &old).map_err(|e| {
                    invalid(format!("the installed copy couldn't be replaced: {e}"))
                })?;
                if let Err(e) = std::fs::rename(&tmp, &dir) {
                    let _ = std::fs::rename(&old, &dir);
                    return Err(invalid(format!("the app couldn't be saved: {e}")));
                }
                let _ = std::fs::remove_dir_all(&old);
            } else {
                std::fs::rename(&tmp, &dir)
                    .map_err(|e| invalid(format!("the app couldn't be saved: {e}")))?;
            }
            Ok(own_app(&id, &dir))
        })
        .await
        .map_err(|e| invalid(format!("writing the app failed: {e}")))?
    }

    /// Makes an editable copy of an app under “My apps”.
    pub async fn clone_app(&self, id: &str) -> Result<UtilityApp, AgentError> {
        let files = self.raw_files(id).await?;
        let manifest = manifest_of(&files).map_err(invalid)?;
        let name = format!("{} (copy)", manifest.name);
        let new_id = self.free_id(&name);
        let mut copy = manifest.clone();
        copy.id = new_id.clone();
        copy.name = name;
        let json = serde_json::to_vec_pretty(&copy)
            .map_err(|e| invalid(format!("the manifest couldn't be written: {e}")))?;
        let files = files
            .into_iter()
            .map(|(p, b)| {
                if p == APP_MANIFEST {
                    (p, json.clone())
                } else {
                    (p, b)
                }
            })
            .collect();
        self.write_app(&new_id, files).await
    }

    /// Every file of an app, as stored.
    async fn raw_files(&self, id: &str) -> Result<Vec<(String, Vec<u8>)>, AgentError> {
        if let Some(b) = builtin::find(id) {
            return Ok(b.raw_files());
        }
        let dir = self.own_dir(id)?;
        tokio::task::spawn_blocking(move || {
            if !dir.is_dir() {
                return Err(AgentError::NotFound("app"));
            }
            read_folder(&dir).map_err(invalid)
        })
        .await
        .map_err(|e| invalid(format!("reading the app failed: {e}")))?
    }

    /// Saves one file of an own app (the app's Code view). The manifest is checked first.
    pub async fn save_file(
        &self,
        id: &str,
        path: &str,
        content: &str,
    ) -> Result<UtilityApp, AgentError> {
        if Self::is_builtin(id) {
            return Err(invalid(
                "Built-in apps can't be changed. Clone the app first.",
            ));
        }
        if !valid_app_path(path) {
            return Err(invalid(format!("“{path}” isn't a valid file path")));
        }
        if content.len() as u64 > MAX_APP_FILE_BYTES {
            return Err(invalid("The file is larger than 2 MB."));
        }
        if path == APP_MANIFEST {
            let m: AppManifest = serde_json::from_str(content)
                .map_err(|e| invalid(format!("{APP_MANIFEST} isn't valid: {e}")))?;
            m.validate().map_err(invalid)?;
            if m.id != id {
                return Err(invalid("The app's id can't be changed."));
            }
        }
        let dir = self.own_dir(id)?;
        let path = path.to_owned();
        let content = content.to_owned();
        let id = id.to_owned();
        tokio::task::spawn_blocking(move || {
            if !dir.is_dir() {
                return Err(AgentError::NotFound("app"));
            }
            let target = dir.join(&path);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| invalid(format!("can't save {path}: {e}")))?;
            }
            std::fs::write(&target, content)
                .map_err(|e| invalid(format!("can't save {path}: {e}")))?;
            Ok(own_app(&id, &dir))
        })
        .await
        .map_err(|e| invalid(format!("saving failed: {e}")))?
    }

    /// Removes an own app with its settings, storage and keys.
    pub async fn delete(&self, id: &str) -> Result<(), AgentError> {
        if Self::is_builtin(id) {
            return Err(invalid("Built-in apps can't be deleted."));
        }
        let dir = self.own_dir(id)?;
        tokio::task::spawn_blocking(move || {
            if dir.exists() {
                std::fs::remove_dir_all(&dir)
                    .map_err(|e| invalid(format!("the app's folder couldn't be removed: {e}")))?;
            }
            Ok::<_, AgentError>(())
        })
        .await
        .map_err(|e| invalid(format!("deleting failed: {e}")))??;
        self.inner.store.delete_app_data(id).await?;
        self.forget_keys(id).await;
        Ok(())
    }

    async fn forget_keys(&self, id: &str) {
        let secrets = self.inner.secrets.clone();
        let names: Vec<String> = GenerationKind::ALL
            .into_iter()
            .map(|k| app_secret_name(id, k))
            .collect();
        let _ = tokio::task::spawn_blocking(move || {
            for n in names {
                let _ = secrets.delete(&n);
            }
        })
        .await;
    }

    /// The `.oxideapp` file of an app: a zip of its files.
    pub async fn export_bytes(&self, id: &str) -> Result<Vec<u8>, AgentError> {
        let files = self.raw_files(id).await?;
        manifest_of(&files).map_err(invalid)?;
        let now = i64::try_from(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        )
        .unwrap_or(0);
        let mut zip = oxide_mcp::archive::ZipWriter::new(Vec::new(), now);
        for (path, bytes) in &files {
            zip.add(path, bytes)
                .map_err(|e| invalid(format!("the export couldn't be written: {e}")))?;
        }
        zip.finish()
            .map_err(|e| invalid(format!("the export couldn't be written: {e}")))
    }

    /// Reads an `.oxideapp` file and keeps it until [`UtilityApps::install_import`] (nothing
    /// is installed yet).
    pub async fn preview_import(&self, bytes: Vec<u8>) -> Result<AppImportPreview, AgentError> {
        let files = tokio::task::spawn_blocking(move || unzip_app(&bytes))
            .await
            .map_err(|e| invalid(format!("reading the file failed: {e}")))?
            .map_err(|e| invalid(format!("This isn't an app Oxide can import: {e}.")))?;
        let manifest = manifest_of(&files)
            .map_err(|e| invalid(format!("This isn't an app Oxide can import: {e}.")))?;
        let renamed_from = Self::is_builtin(&manifest.id).then(|| manifest.id.clone());
        let replaces = renamed_from.is_none() && self.inner.dir.join(&manifest.id).is_dir();
        let bytes: u64 = files.iter().map(|(_, b)| b.len() as u64).sum();
        let token = uuid::Uuid::now_v7().to_string();
        let preview = AppImportPreview {
            token: token.clone(),
            manifest: manifest.clone(),
            files: files.len() as u32,
            bytes: u32::try_from(bytes).unwrap_or(u32::MAX),
            replaces,
            renamed_from,
        };
        let mut imports = lock(&self.inner.imports);
        imports.retain(|_, p| p.at.elapsed() < IMPORT_TTL);
        imports.insert(
            token,
            PendingImport {
                manifest,
                files,
                at: Instant::now(),
            },
        );
        Ok(preview)
    }

    /// Installs an import that [`UtilityApps::preview_import`] read. An app with a built-in
    /// app's id gets a new id; an own app with the same id is replaced (its settings and
    /// grants are forgotten, since the code changed).
    pub async fn install_import(&self, token: &str) -> Result<UtilityApp, AgentError> {
        let pending = lock(&self.inner.imports)
            .remove(token)
            .ok_or_else(|| invalid("The import expired. Choose the file again."))?;
        let mut manifest = pending.manifest;
        let mut files = pending.files;
        if Self::is_builtin(&manifest.id) {
            manifest.id = self.free_id(&manifest.name);
            let json = serde_json::to_vec_pretty(&manifest)
                .map_err(|e| invalid(format!("the manifest couldn't be written: {e}")))?;
            for f in files.iter_mut() {
                if f.0 == APP_MANIFEST {
                    f.1 = json.clone();
                }
            }
        } else {
            self.inner.store.delete_app_data(&manifest.id).await?;
            self.forget_keys(&manifest.id).await;
        }
        self.write_app(&manifest.id, files).await
    }

    // ---- settings, grants, keys, storage ----

    pub async fn prefs(&self, id: &str) -> Result<AppPrefs, AgentError> {
        Ok(self.inner.store.app_prefs(id).await?)
    }

    /// Saves the settings dialog. Grants can only be removed here (they are added by
    /// [`UtilityApps::grant`], after the user allowed them).
    pub async fn save_prefs(&self, id: &str, mut prefs: AppPrefs) -> Result<AppPrefs, AgentError> {
        let old = self.inner.store.app_prefs(id).await?;
        prefs.granted.retain(|c| old.granted.contains(c));
        prefs.text_model = prefs.text_model.filter(|m| !m.trim().is_empty());
        self.inner.store.save_app_prefs(id, &prefs).await?;
        Ok(prefs)
    }

    /// The user allowed a capability for this app.
    pub async fn grant(&self, id: &str, cap: AppCapability) -> Result<AppPrefs, AgentError> {
        let manifest = self.manifest(id).await?;
        if !manifest.declares(cap) {
            return Err(invalid(format!(
                "{} doesn't declare “{}” in its manifest.",
                manifest.name,
                cap.as_str()
            )));
        }
        let mut prefs = self.inner.store.app_prefs(id).await?;
        if !prefs.granted.contains(&cap) {
            prefs.granted.push(cap);
            prefs.granted.sort();
        }
        self.inner.store.save_app_prefs(id, &prefs).await?;
        Ok(prefs)
    }

    /// Kinds of generation with an app-own key.
    pub async fn keys(&self, id: &str) -> Vec<GenerationKind> {
        let secrets = self.inner.secrets.clone();
        let id = id.to_owned();
        tokio::task::spawn_blocking(move || {
            GenerationKind::ALL
                .into_iter()
                .filter(|k| matches!(secrets.get(&app_secret_name(&id, *k)), Ok(Some(_))))
                .collect()
        })
        .await
        .unwrap_or_default()
    }

    /// Sets (or with `None` or empty, removes) an app's own key for a kind of generation.
    pub async fn set_key(
        &self,
        id: &str,
        kind: GenerationKind,
        key: Option<String>,
    ) -> Result<(), AgentError> {
        self.manifest(id).await?;
        let secrets = self.inner.secrets.clone();
        let name = app_secret_name(id, kind);
        tokio::task::spawn_blocking(move || {
            match key.as_deref().map(str::trim).filter(|k| !k.is_empty()) {
                Some(k) => secrets.set(&name, &Secret::new(k)),
                None => secrets.delete(&name),
            }
        })
        .await
        .map_err(|e| invalid(format!("the keychain failed: {e}")))?
        .map_err(|e| invalid(format!("The key couldn't be saved: {e}")))
    }

    pub async fn clear_storage(&self, id: &str) -> Result<(), AgentError> {
        Ok(self.inner.store.app_storage_clear(id).await?)
    }
}

/// An own app's gallery entry; a broken folder is listed with its problem.
fn own_app(id: &str, dir: &Path) -> UtilityApp {
    let folder = Some(dir.to_string_lossy().into_owned());
    let fallback = |problem: String| UtilityApp {
        manifest: AppManifest {
            id: id.to_owned(),
            name: id.to_owned(),
            description: String::new(),
            icon: "triangle-alert".into(),
            category: "Other".into(),
            version: "0.0.0".into(),
            entry: "App.tsx".into(),
            capabilities: Vec::new(),
            models: Default::default(),
            author: None,
        },
        builtin: false,
        folder: folder.clone(),
        problem: Some(problem),
    };
    let raw = match std::fs::read(dir.join(APP_MANIFEST)) {
        Ok(r) => r,
        Err(_) => return fallback(format!("{APP_MANIFEST} is missing")),
    };
    let manifest: AppManifest = match serde_json::from_slice(&raw) {
        Ok(m) => m,
        Err(e) => return fallback(format!("{APP_MANIFEST} isn't valid: {e}")),
    };
    let mut problem = manifest.validate().err();
    if problem.is_none() && manifest.id != id {
        problem = Some(format!(
            "the manifest's id “{}” doesn't match the folder “{id}”",
            manifest.id
        ));
    }
    if problem.is_none() && !dir.join(&manifest.entry).is_file() {
        problem = Some(format!("the entry file {} is missing", manifest.entry));
    }
    UtilityApp {
        manifest,
        builtin: false,
        folder,
        problem,
    }
}

/// An `.oxideapp`'s files. The archive may hold the files at its root or in one folder.
fn unzip_app(bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>, String> {
    use oxide_mcp::archive::{read_entries, read_entry};
    let mut cursor = Cursor::new(bytes);
    let entries = read_entries(&mut cursor).map_err(|e| e.to_string())?;
    let files: Vec<_> = entries.iter().filter(|e| !e.is_dir()).collect();
    if files.len() > MAX_APP_FILES {
        return Err("it has more than 200 files".into());
    }
    // One top folder holding the manifest: strip it.
    let prefix = if files.iter().any(|e| e.name == APP_MANIFEST) {
        String::new()
    } else {
        files
            .iter()
            .find(|e| {
                e.name.ends_with(&format!("/{APP_MANIFEST}")) && e.name.matches('/').count() == 1
            })
            .map(|e| e.name.trim_end_matches(APP_MANIFEST).to_owned())
            .unwrap_or_default()
    };
    let mut out = Vec::new();
    let mut total = 0u64;
    for e in files {
        let Some(rel) = e.name.strip_prefix(&prefix) else {
            continue;
        };
        if !valid_app_path(rel) {
            return Err(format!("it has a file with an unsafe path ({})", e.name));
        }
        if e.size > MAX_APP_FILE_BYTES {
            return Err(format!("{rel} is larger than 2 MB"));
        }
        total += e.size;
        if total > MAX_APP_BYTES {
            return Err("it is larger than 20 MB".into());
        }
        let data = read_entry(&mut cursor, e, MAX_APP_FILE_BYTES).map_err(|e| e.to_string())?;
        out.push((rel.to_owned(), data));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

#[cfg(test)]
mod tests;
