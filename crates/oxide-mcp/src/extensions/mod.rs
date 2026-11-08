//! Desktop extensions (`.mcpb`): open and check a bundle, install it into Oxide's data folder,
//! keep its settings (sensitive ones in the keychain), and run its server through the
//! [`McpManager`] like any other connector.
//!
//! Layout: `<extensions dir>/<name>/` holds the unpacked bundle; `<extensions dir>/.staging/`
//! holds bundles that were opened but not installed yet. The managed entry lives in
//! `oxide_config.json` under `oxide.extensions.<name>` (see [`crate::config`]).

pub mod manifest;

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use base64::Engine;
use oxide_core::extension::platform_name;
use oxide_core::{
    Extension, ExtensionField, ExtensionFieldType, ExtensionPreview, ExtensionServerType,
    ExtensionStatus, ExtensionValue,
};

use crate::McpError;
use crate::archive::{self, ZipLimits};
use crate::config::ExtensionEntry;
use crate::log::now_ms;
use crate::manager::{ExtensionServer, McpManager};
use crate::oauth::TokenStore;
use crate::process;
pub use manifest::{Manifest, UserDirs};

/// The manifest's file name inside a bundle.
pub const MANIFEST_FILE: &str = "manifest.json";
const STAGING_DIR: &str = ".staging";
/// Bundles opened but not installed are cleaned up after this long.
const STAGING_TTL: Duration = Duration::from_secs(60 * 60);
/// Largest icon shown in Settings.
const MAX_ICON_BYTES: u64 = 512 * 1024;
/// Largest manifest Oxide reads.
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;

fn bad(msg: impl Into<String>) -> McpError {
    McpError::Extension(msg.into())
}

/// Keychain entry of one sensitive setting.
pub fn secret_key(extension: &str, key: &str) -> String {
    format!("extension:{extension}:{key}")
}

struct Staged {
    dir: PathBuf,
    manifest: Manifest,
    opened: Instant,
}

/// Why an installed extension can't run.
#[derive(Debug, Clone)]
struct Problem {
    status: ExtensionStatus,
    message: String,
}

struct Inner {
    root: PathBuf,
    manager: McpManager,
    tokens: Arc<dyn TokenStore>,
    dirs: UserDirs,
    platform: String,
    staged: Mutex<HashMap<String, Staged>>,
    problems: Mutex<HashMap<String, Problem>>,
    /// Oxide's own `uv` (Phase 12), preferred over one on the PATH.
    uv: Mutex<Option<PathBuf>>,
    /// One install, update or uninstall at a time.
    busy: tokio::sync::Mutex<()>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Installed desktop extensions. Cheap to clone.
#[derive(Clone)]
pub struct Extensions {
    inner: Arc<Inner>,
}

impl Extensions {
    /// Extensions under `root`, run by `manager`, with sensitive settings in `tokens`.
    pub fn new(root: PathBuf, manager: McpManager, tokens: Arc<dyn TokenStore>) -> Self {
        Self::with(
            root,
            manager,
            tokens,
            UserDirs::current(),
            platform_name(std::env::consts::OS).to_owned(),
        )
    }

    /// Like [`Extensions::new`], with the user folders and platform given (tests).
    pub fn with(
        root: PathBuf,
        manager: McpManager,
        tokens: Arc<dyn TokenStore>,
        dirs: UserDirs,
        platform: String,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                root,
                manager,
                tokens,
                dirs,
                platform,
                staged: Mutex::new(HashMap::new()),
                problems: Mutex::new(HashMap::new()),
                uv: Mutex::new(None),
                busy: tokio::sync::Mutex::new(()),
            }),
        }
    }

    pub fn root(&self) -> &Path {
        &self.inner.root
    }

    /// Sets Oxide's own `uv`, used for Python and uv extensions before any `uv` on the PATH.
    /// Call [`Extensions::refresh`] afterwards if servers were already built.
    pub fn set_uv(&self, uv: Option<PathBuf>) {
        *lock(&self.inner.uv) = uv;
    }

    /// The `uv` extensions run with: Oxide's own, else one on the PATH.
    fn uv(&self) -> Option<String> {
        lock(&self.inner.uv)
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned())
            .or_else(|| find_program("uv"))
    }

    fn dir_of(&self, name: &str) -> PathBuf {
        self.inner.root.join(name)
    }

    /// Builds every installed extension's server and hands them to the manager. Call after
    /// the manager has started, and after anything changes.
    pub async fn refresh(&self) {
        let cfg = self.inner.manager.config_snapshot();
        let uv = self.uv();
        let mut servers = HashMap::new();
        let mut problems = HashMap::new();
        for (name, entry) in cfg.extensions() {
            match self.build(&name, &entry, uv.as_deref()).await {
                Ok(server) => {
                    servers.insert(name, server);
                }
                Err(p) => {
                    problems.insert(name, p);
                }
            }
        }
        *lock(&self.inner.problems) = problems;
        self.inner.manager.set_extension_servers(servers);
    }

    async fn build(
        &self,
        name: &str,
        entry: &ExtensionEntry,
        uv: Option<&str>,
    ) -> Result<ExtensionServer, Problem> {
        let dir = self.dir_of(name);
        let m = read_manifest(&dir, &self.inner.dirs).map_err(|e| Problem {
            status: ExtensionStatus::Broken,
            message: format!("{e}. Reinstall the extension."),
        })?;
        if !m.supports(&self.inner.platform) {
            return Err(Problem {
                status: ExtensionStatus::Unsupported,
                message: format!(
                    "It's made for {}, not this computer.",
                    platforms_text(&m.info.platforms)
                ),
            });
        }
        let mut values = entry.values.clone();
        for f in m.info.fields.iter().filter(|f| f.sensitive) {
            match self.inner.tokens.load(&secret_key(name, &f.key)).await {
                Ok(Some(v)) => {
                    values.insert(f.key.clone(), ExtensionValue::Text(v));
                }
                Ok(None) => {}
                Err(e) => {
                    return Err(Problem {
                        status: ExtensionStatus::Broken,
                        message: format!("The keychain couldn't be read: {e}"),
                    });
                }
            }
        }
        let launch = manifest::launch(
            &m,
            &dir,
            &values,
            &self.inner.platform,
            &self.inner.dirs,
            uv,
        )
        .map_err(|missing| Problem {
            status: ExtensionStatus::NeedsSetup,
            message: format!("Fill in “{}” in its settings.", missing.0),
        })?;
        Ok(ExtensionServer {
            command: launch.command,
            args: launch.args,
            env: launch.env,
            summary: launch.summary,
            display_name: m.info.display_name.clone(),
            dir,
        })
    }

    /// Installed extensions, by display name.
    pub async fn list(&self) -> Vec<Extension> {
        let cfg = self.inner.manager.config_snapshot();
        let problems = lock(&self.inner.problems).clone();
        let mut out = Vec::new();
        for (name, entry) in cfg.extensions() {
            let dir = self.dir_of(&name);
            let info = read_manifest(&dir, &self.inner.dirs).ok().map(|m| {
                let mut info = m.info.clone();
                info.icon = read_icon(&dir, m.icon_path.as_deref());
                info
            });
            let problem = problems.get(&name);
            out.push(Extension {
                version: info
                    .as_ref()
                    .map(|i| i.version.clone())
                    .unwrap_or_else(|| entry.version.clone()),
                status: match (&info, problem) {
                    (None, _) => ExtensionStatus::Broken,
                    (_, Some(p)) => p.status,
                    _ => ExtensionStatus::Ready,
                },
                problem: match (&info, problem) {
                    (None, _) => Some("Its files can't be read. Reinstall the extension.".into()),
                    (_, p) => p.map(|p| p.message.clone()),
                },
                info,
                dir: dir.display().to_string(),
                enabled: entry.enabled,
                values: entry.values.clone(),
                secrets_set: entry.secrets.clone(),
                installed_at: entry.installed_at,
                name,
            });
        }
        out.sort_by_key(|e| {
            e.info
                .as_ref()
                .map(|i| i.display_name.to_lowercase())
                .unwrap_or_else(|| e.name.to_lowercase())
        });
        out
    }

    pub async fn get(&self, name: &str) -> Result<Extension, McpError> {
        self.list()
            .await
            .into_iter()
            .find(|e| e.name == name)
            .ok_or_else(|| bad(format!("there's no extension called \"{name}\"")))
    }

    /// Opens and checks a `.mcpb` file without installing it.
    pub async fn preview(&self, file: &Path) -> Result<ExtensionPreview, McpError> {
        self.clean_staging().await;
        let id = staging_id();
        let staging = self.inner.root.join(STAGING_DIR).join(&id);
        let (src, dest) = (file.to_path_buf(), staging.clone());
        let unpacked = tokio::task::spawn_blocking(move || {
            archive::extract_zip(&src, &dest, ZipLimits::default())
        })
        .await
        .map_err(|e| bad(format!("unpacking failed: {e}")))?;
        let unpacked = match unpacked {
            Ok(u) => u,
            Err(e) => {
                remove_dir(&staging).await;
                return Err(e);
            }
        };
        let checked = read_manifest(&staging, &self.inner.dirs)
            .and_then(|m| m.check_files(&staging).map(|()| m));
        let m = match checked {
            Ok(m) => m,
            Err(e) => {
                remove_dir(&staging).await;
                return Err(e);
            }
        };
        let name = m.info.name.clone();
        let cfg = self.inner.manager.config_snapshot();
        if cfg.has_server(&name) {
            remove_dir(&staging).await;
            return Err(bad(format!(
                "a connector called \"{name}\" is already set up in Settings → Connectors; \
                 rename or remove it first"
            )));
        }
        let existing = cfg.extension(&name);
        let mut warnings = m.warnings.clone();
        if !m.supports(&self.inner.platform) {
            warnings.push(format!(
                "It's made for {}, so it won't run on this computer.",
                platforms_text(&m.info.platforms)
            ));
        }
        warnings.extend(runtime_warnings(&m, self.uv().is_some()));
        let mut values: BTreeMap<String, ExtensionValue> = m
            .info
            .fields
            .iter()
            .filter(|f| !f.sensitive)
            .filter_map(|f| Some((f.key.clone(), f.default.clone()?)))
            .collect();
        let mut secrets_set = Vec::new();
        if let Some(e) = &existing {
            for (k, v) in &e.values {
                if m.info.fields.iter().any(|f| &f.key == k && !f.sensitive) {
                    values.insert(k.clone(), v.clone());
                }
            }
            secrets_set = e
                .secrets
                .iter()
                .filter(|k| m.info.fields.iter().any(|f| &f.key == *k && f.sensitive))
                .cloned()
                .collect();
        }
        let mut info = m.info.clone();
        info.icon = read_icon(&staging, m.icon_path.as_deref());
        lock(&self.inner.staged).insert(
            id.clone(),
            Staged {
                dir: staging,
                manifest: m,
                opened: Instant::now(),
            },
        );
        Ok(ExtensionPreview {
            staging_id: id,
            info,
            installed_version: existing.map(|e| e.version),
            warnings,
            values,
            secrets_set,
            size: unpacked.bytes,
            files: unpacked.files,
        })
    }

    /// Forgets a bundle that was opened but won't be installed.
    pub async fn discard(&self, staging_id: &str) {
        let staged = lock(&self.inner.staged).remove(staging_id);
        if let Some(s) = staged {
            remove_dir(&s.dir).await;
        }
    }

    /// Installs (or updates) an opened bundle with the user's settings.
    pub async fn install(
        &self,
        staging_id: &str,
        values: BTreeMap<String, ExtensionValue>,
    ) -> Result<Extension, McpError> {
        let _busy = self.inner.busy.lock().await;
        let staged = lock(&self.inner.staged)
            .remove(staging_id)
            .ok_or_else(|| bad("open the extension file again (it was closed)"))?;
        let name = staged.manifest.info.name.clone();
        let fields = staged.manifest.info.fields.clone();
        if let Err(e) = check_values(&fields, &values) {
            // Keep it open so the form can be fixed.
            lock(&self.inner.staged).insert(staging_id.to_owned(), staged);
            return Err(e);
        }
        let cfg = self.inner.manager.config_snapshot();
        let mut entry = cfg.extension(&name).unwrap_or_default();
        let existed = cfg.is_extension(&name);

        // Swap the folders: stop the running server first, since Windows keeps its files open.
        let target = self.dir_of(&name);
        self.inner.manager.drop_extension_server(&name);
        if target.exists() {
            let old = self
                .inner
                .root
                .join(STAGING_DIR)
                .join(format!("{}-old", staging_id));
            rename_retry(&target, &old).await.map_err(|e| {
                bad(format!(
                    "the installed version couldn't be replaced ({e}); close anything using its \
                     files and try again"
                ))
            })?;
            remove_dir(&old).await;
        }
        if let Some(parent) = target.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| bad(format!("couldn't create the extensions folder: {e}")))?;
        }
        rename_retry(&staged.dir, &target)
            .await
            .map_err(|e| bad(format!("couldn't install the extension: {e}")))?;

        self.store_values(&name, &fields, values, &mut entry)
            .await?;
        entry.version = staged.manifest.info.version.clone();
        if !existed {
            entry.enabled = true;
            entry.installed_at = now_ms();
        }
        self.inner
            .manager
            .edit(|c| {
                c.set_extension(&name, &entry);
                Ok(())
            })
            .await?;
        self.refresh().await;
        self.get(&name).await
    }

    /// Saves new settings for an installed extension (its server restarts with them).
    pub async fn configure(
        &self,
        name: &str,
        values: BTreeMap<String, ExtensionValue>,
    ) -> Result<Extension, McpError> {
        let _busy = self.inner.busy.lock().await;
        let cfg = self.inner.manager.config_snapshot();
        let mut entry = cfg
            .extension(name)
            .ok_or_else(|| bad(format!("there's no extension called \"{name}\"")))?;
        let m = read_manifest(&self.dir_of(name), &self.inner.dirs)?;
        check_values(&m.info.fields, &values)?;
        self.store_values(name, &m.info.fields, values, &mut entry)
            .await?;
        self.inner
            .manager
            .edit(|c| {
                c.set_extension(name, &entry);
                Ok(())
            })
            .await?;
        self.refresh().await;
        self.get(name).await
    }

    /// Splits values: sensitive ones go to the keychain (an empty one keeps what's stored),
    /// the rest into the entry.
    async fn store_values(
        &self,
        name: &str,
        fields: &[ExtensionField],
        values: BTreeMap<String, ExtensionValue>,
        entry: &mut ExtensionEntry,
    ) -> Result<(), McpError> {
        let mut plain = BTreeMap::new();
        let mut secrets: Vec<String> = entry
            .secrets
            .iter()
            .filter(|k| fields.iter().any(|f| &f.key == *k && f.sensitive))
            .cloned()
            .collect();
        for f in fields {
            let Some(v) = values.get(&f.key) else {
                continue;
            };
            if f.sensitive {
                let text = v.parts().join(",");
                if text.trim().is_empty() {
                    continue;
                }
                self.inner
                    .tokens
                    .save(&secret_key(name, &f.key), text)
                    .await
                    .map_err(|e| bad(format!("the keychain couldn't save “{}”: {e}", f.title)))?;
                if !secrets.contains(&f.key) {
                    secrets.push(f.key.clone());
                }
            } else if !v.is_empty() {
                plain.insert(f.key.clone(), v.clone());
            }
        }
        entry.values = plain;
        entry.secrets = secrets;
        Ok(())
    }

    /// Turns an extension's server on or off.
    pub async fn set_enabled(&self, name: &str, enabled: bool) -> Result<(), McpError> {
        if !self.inner.manager.config_snapshot().is_extension(name) {
            return Err(bad(format!("there's no extension called \"{name}\"")));
        }
        self.inner.manager.set_enabled(name, enabled).await
    }

    /// Stops and removes an extension: its files, settings and keychain entries.
    pub async fn uninstall(&self, name: &str) -> Result<(), McpError> {
        let _busy = self.inner.busy.lock().await;
        let cfg = self.inner.manager.config_snapshot();
        let entry = cfg
            .extension(name)
            .ok_or_else(|| bad(format!("there's no extension called \"{name}\"")))?;
        self.inner.manager.drop_extension_server(name);
        self.inner
            .manager
            .edit(|c| {
                c.remove_extension(name);
                Ok(())
            })
            .await?;
        for key in &entry.secrets {
            if let Err(e) = self.inner.tokens.delete(&secret_key(name, key)).await {
                tracing::warn!("couldn't delete an extension secret: {e}");
            }
        }
        lock(&self.inner.problems).remove(name);
        let dir = self.dir_of(name);
        if dir.exists() {
            // Move it aside first (that works once the server has let go), then delete.
            let aside = self.inner.root.join(STAGING_DIR).join(format!(
                "{}-removed-{}",
                name,
                staging_id()
            ));
            match rename_retry(&dir, &aside).await {
                Ok(()) => remove_dir(&aside).await,
                Err(e) => tracing::warn!("couldn't remove {}: {e}", dir.display()),
            }
        }
        Ok(())
    }

    /// Removes leftovers of bundles that were opened long ago and never installed.
    async fn clean_staging(&self) {
        let old: Vec<(String, PathBuf)> = {
            let mut staged = lock(&self.inner.staged);
            let ids: Vec<String> = staged
                .iter()
                .filter(|(_, s)| s.opened.elapsed() > STAGING_TTL)
                .map(|(k, _)| k.clone())
                .collect();
            ids.into_iter()
                .filter_map(|k| staged.remove(&k).map(|s| (k, s.dir)))
                .collect()
        };
        for (_, dir) in old {
            remove_dir(&dir).await;
        }
        // Folders from an earlier run that nothing refers to.
        let staging = self.inner.root.join(STAGING_DIR);
        let live: Vec<PathBuf> = lock(&self.inner.staged)
            .values()
            .map(|s| s.dir.clone())
            .collect();
        if let Ok(mut rd) = tokio::fs::read_dir(&staging).await {
            while let Ok(Some(e)) = rd.next_entry().await {
                let p = e.path();
                let stale = e
                    .metadata()
                    .await
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.elapsed().ok())
                    .is_some_and(|age| age > STAGING_TTL);
                if stale && !live.contains(&p) {
                    remove_dir(&p).await;
                }
            }
        }
    }
}

/// Reads and parses `<dir>/manifest.json`.
pub fn read_manifest(dir: &Path, dirs: &UserDirs) -> Result<Manifest, McpError> {
    let path = dir.join(MANIFEST_FILE);
    let meta = std::fs::metadata(&path)
        .map_err(|_| bad("the bundle has no manifest.json at its top level"))?;
    if meta.len() > MAX_MANIFEST_BYTES {
        return Err(bad("manifest.json is too large"));
    }
    let text = std::fs::read_to_string(&path)
        .map_err(|e| bad(format!("couldn't read manifest.json: {e}")))?;
    manifest::parse(&text, dirs)
}

/// The icon as a `data:` URL, if the bundle has a small PNG, JPEG or SVG one.
fn read_icon(dir: &Path, icon: Option<&str>) -> Option<String> {
    let rel = archive::safe_path(icon?).ok().flatten()?;
    let path = dir.join(rel);
    if std::fs::metadata(&path).ok()?.len() > MAX_ICON_BYTES {
        return None;
    }
    let mime = match path
        .extension()?
        .to_string_lossy()
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "svg" => "image/svg+xml",
        _ => return None,
    };
    let bytes = std::fs::read(&path).ok()?;
    Some(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

fn platforms_text(p: &[String]) -> String {
    let names: Vec<&str> = p
        .iter()
        .map(|p| match p.as_str() {
            "win32" => "Windows",
            "darwin" => "macOS",
            "linux" => "Linux",
            other => other,
        })
        .collect();
    match names.len() {
        0 => "other systems".into(),
        1 => names[0].to_owned(),
        _ => {
            let (last, rest) = names.split_last().unwrap_or((&"", &[]));
            format!("{} and {last}", rest.join(", "))
        }
    }
}

/// Finds a program the way server processes will (PATH plus the usual install folders).
fn find_program(name: &str) -> Option<String> {
    let path = process::child_path(&BTreeMap::new());
    process::resolve_command(name, &path)
        .ok()
        .map(|p| p.to_string_lossy().into_owned())
}

/// What the extension needs that this computer may not have.
fn runtime_warnings(m: &Manifest, has_uv: bool) -> Vec<String> {
    let want = |k: &str| {
        m.info
            .runtimes
            .get(k)
            .map(|v| format!(" ({v})"))
            .unwrap_or_default()
    };
    match m.info.server_type {
        ExtensionServerType::Node if find_program("node").is_none() => vec![format!(
            "It needs Node.js{}, which wasn't found. Install it from https://nodejs.org, then \
             restart Oxide.",
            want("node")
        )],
        ExtensionServerType::Python if !has_uv => {
            if find_program("python").is_none() && find_program("python3").is_none() {
                vec![format!(
                    "It needs Python{}. Install uv from https://docs.astral.sh/uv/ (Oxide uses it \
                     to run Python extensions), then restart Oxide.",
                    want("python")
                )]
            } else {
                vec![
                    "uv wasn't found, so it runs with the system's Python. Install uv from \
                     https://docs.astral.sh/uv/ if the version doesn't fit."
                        .into(),
                ]
            }
        }
        ExtensionServerType::Uv if !has_uv => vec![
            "It needs uv, which wasn't found. Install it from https://docs.astral.sh/uv/, then \
             restart Oxide."
                .into(),
        ],
        _ => Vec::new(),
    }
}

/// Checks the form's values against the fields (types, ranges).
pub fn check_values(
    fields: &[ExtensionField],
    values: &BTreeMap<String, ExtensionValue>,
) -> Result<(), McpError> {
    for f in fields {
        let Some(v) = values.get(&f.key) else {
            continue;
        };
        let ok = match (f.kind, v) {
            (ExtensionFieldType::Boolean, ExtensionValue::Bool(_)) => true,
            (ExtensionFieldType::Number, ExtensionValue::Number(n)) => {
                if let Some(min) = f.min
                    && *n < min
                {
                    return Err(bad(format!(
                        "“{}” must be at least {}",
                        f.title,
                        oxide_core::extension::format_number(min)
                    )));
                }
                if let Some(max) = f.max
                    && *n > max
                {
                    return Err(bad(format!(
                        "“{}” must be at most {}",
                        f.title,
                        oxide_core::extension::format_number(max)
                    )));
                }
                n.is_finite()
            }
            (ExtensionFieldType::Number, ExtensionValue::Text(t)) => t.trim().is_empty(),
            (ExtensionFieldType::Directory | ExtensionFieldType::File, ExtensionValue::List(_)) => {
                f.multiple
            }
            (
                ExtensionFieldType::String
                | ExtensionFieldType::Directory
                | ExtensionFieldType::File,
                ExtensionValue::Text(_),
            ) => true,
            _ => false,
        };
        if !ok {
            return Err(bad(format!("“{}” has the wrong kind of value", f.title)));
        }
    }
    Ok(())
}

fn staging_id() -> String {
    use rand::Rng;
    let n: u64 = rand::thread_rng().r#gen();
    format!("{:x}{n:016x}", now_ms())
}

/// Renames, retrying for a few seconds (Windows refuses while a stopped server's process is
/// still exiting).
async fn rename_retry(from: &Path, to: &Path) -> std::io::Result<()> {
    let mut last = None;
    for _ in 0..20 {
        match tokio::fs::rename(from, to).await {
            Ok(()) => return Ok(()),
            Err(e) => last = Some(e),
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    Err(last.unwrap_or_else(|| std::io::Error::other("rename failed")))
}

async fn remove_dir(dir: &Path) {
    for _ in 0..8 {
        match tokio::fs::remove_dir_all(dir).await {
            Ok(()) => return,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
            Err(_) => tokio::time::sleep(Duration::from_millis(250)).await,
        }
    }
    tracing::warn!("couldn't delete {}", dir.display());
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxide_core::ExtensionFieldType as T;

    fn field(key: &str, kind: T) -> ExtensionField {
        ExtensionField {
            key: key.into(),
            kind,
            title: key.to_uppercase(),
            description: None,
            required: false,
            default: None,
            multiple: false,
            sensitive: false,
            min: Some(1.0),
            max: Some(10.0),
        }
    }

    #[test]
    fn checks_values() {
        let fields = vec![
            field("n", T::Number),
            field("b", T::Boolean),
            field("d", T::Directory),
        ];
        let ok: BTreeMap<String, ExtensionValue> = [
            ("n".to_string(), ExtensionValue::Number(5.0)),
            ("b".to_string(), ExtensionValue::Bool(true)),
            ("d".to_string(), ExtensionValue::Text("/x".into())),
            (
                "unknown".to_string(),
                ExtensionValue::Text("ignored".into()),
            ),
        ]
        .into();
        check_values(&fields, &ok).expect("ok");
        let too_big = [("n".to_string(), ExtensionValue::Number(11.0))].into();
        let err = check_values(&fields, &too_big).expect_err("max");
        assert!(err.to_string().contains("at most 10"), "{err}");
        let small = [("n".to_string(), ExtensionValue::Number(0.0))].into();
        assert!(check_values(&fields, &small).is_err());
        let wrong = [("b".to_string(), ExtensionValue::Text("yes".into()))].into();
        assert!(check_values(&fields, &wrong).is_err());
        let list = [("d".to_string(), ExtensionValue::List(vec!["/a".into()]))].into();
        assert!(
            check_values(&fields, &list).is_err(),
            "not a multi-pick field"
        );
    }

    #[test]
    fn names_platforms() {
        assert_eq!(platforms_text(&["darwin".into()]), "macOS");
        assert_eq!(
            platforms_text(&["darwin".into(), "linux".into(), "win32".into()]),
            "macOS, Linux and Windows"
        );
        assert_eq!(secret_key("files", "key"), "extension:files:key");
    }
}
