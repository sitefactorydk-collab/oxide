//! `oxide_config.json`: MCP servers in Claude desktop's `mcpServers` schema, so entries paste
//! 1:1, plus Oxide-only settings under a top-level `"oxide"` object (never inside an entry).
//!
//! ```json
//! {
//!   "mcpServers": {
//!     "filesystem": { "command": "npx", "args": ["-y", "@modelcontextprotocol/server-filesystem", "C:\\Users\\me"] },
//!     "docs": { "type": "http", "url": "https://example.com/mcp", "headers": { "X-Key": "…" } }
//!   },
//!   "oxide": {
//!     "disabledServers": ["docs"],
//!     "servers": { "docs": { "oauthClientId": "…" } },
//!     "extensions": { "files": { "enabled": true, "version": "1.2.0", "values": { … } } }
//!   }
//! }
//! ```
//!
//! Unknown keys anywhere are kept when Oxide rewrites the file.
//!
//! Desktop extensions are managed entries under `oxide.extensions`, not in `mcpServers`: their
//! command line is built from the bundle's manifest when Oxide starts (it needs the install
//! folder and secrets from the keychain), so it isn't something to paste elsewhere.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use oxide_core::{
    ExtensionValue, ImportCandidate, ImportPreview, ImportStatus, McpConfigFile, McpServerConfig,
    McpTransport, Timestamp,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::McpError;

pub const CONFIG_FILE_NAME: &str = "oxide_config.json";
/// Longest server name.
const MAX_NAME_CHARS: usize = 64;
/// Keys of an entry that Oxide manages; anything else in an entry is left alone.
const MANAGED_KEYS: &[&str] = &["command", "args", "env", "type", "url", "headers"];

/// The whole config document.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct OxideConfig {
    root: Map<String, Value>,
}

impl OxideConfig {
    /// Parses the file's text. Empty text is an empty config.
    pub fn parse(text: &str) -> Result<Self, McpError> {
        if text.trim().is_empty() {
            return Ok(Self::default());
        }
        let value: Value = serde_json::from_str(text).map_err(|e| {
            McpError::Config(format!(
                "{CONFIG_FILE_NAME} isn't valid JSON (line {}, column {}): {e}",
                e.line(),
                e.column()
            ))
        })?;
        let Value::Object(root) = value else {
            return Err(McpError::Config(format!(
                "{CONFIG_FILE_NAME} must contain a JSON object"
            )));
        };
        if let Some(s) = root.get("mcpServers")
            && !s.is_object()
        {
            return Err(McpError::Config("\"mcpServers\" must be an object".into()));
        }
        Ok(Self { root })
    }

    pub fn to_text(&self) -> String {
        let mut root = self.root.clone();
        root.entry("mcpServers")
            .or_insert_with(|| Value::Object(Map::new()));
        let mut text =
            serde_json::to_string_pretty(&Value::Object(root)).unwrap_or_else(|_| "{}".into());
        text.push('\n');
        text
    }

    fn entries(&self) -> Option<&Map<String, Value>> {
        self.root.get("mcpServers").and_then(Value::as_object)
    }

    fn entries_mut(&mut self) -> &mut Map<String, Value> {
        object_at(&mut self.root, "mcpServers")
    }

    fn oxide_mut(&mut self) -> &mut Map<String, Value> {
        object_at(&mut self.root, "oxide")
    }

    fn disabled(&self) -> Vec<String> {
        self.root
            .get("oxide")
            .and_then(|o| o.get("disabledServers"))
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Server names in file order.
    pub fn names(&self) -> Vec<String> {
        self.entries()
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// Every server; entries that can't be read come back as `Err` with the reason.
    pub fn servers(&self) -> Vec<(String, Result<McpServerConfig, String>)> {
        let disabled = self.disabled();
        self.entries()
            .map(|m| {
                m.iter()
                    .map(|(name, entry)| {
                        let cfg = entry_to_config(name, entry).map(|mut c| {
                            c.enabled = !disabled.contains(name);
                            c
                        });
                        (name.clone(), cfg)
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn server(&self, name: &str) -> Option<Result<McpServerConfig, String>> {
        let entry = self.entries()?.get(name)?;
        Some(entry_to_config(name, entry).map(|mut c| {
            c.enabled = !self.disabled().iter().any(|d| d == name);
            c
        }))
    }

    /// Adds or replaces a server. `old_name` renames an existing entry (its Oxide-only
    /// settings move with it).
    pub fn upsert(
        &mut self,
        cfg: &McpServerConfig,
        old_name: Option<&str>,
    ) -> Result<(), McpError> {
        let name = validate(cfg)?;
        let old = old_name.unwrap_or(&name).to_owned();
        if old != name && self.entries().is_some_and(|m| m.contains_key(&name)) {
            return Err(McpError::Config(format!(
                "there's already a server called \"{name}\""
            )));
        }
        let existing = self.entries().and_then(|m| m.get(&old)).cloned();
        let entry = config_to_entry(cfg, existing.as_ref());
        let entries = self.entries_mut();
        if old != name {
            // Keep the position of a renamed entry where the map allows it.
            entries.remove(&old);
        }
        entries.insert(name.clone(), entry);
        if old != name {
            if let Some(Value::Object(servers)) = self.oxide_mut().get_mut("servers")
                && let Some(v) = servers.remove(&old)
            {
                servers.insert(name.clone(), v);
            }
            self.set_enabled(&old, true);
        }
        self.set_enabled(&name, cfg.enabled);
        Ok(())
    }

    pub fn remove(&mut self, name: &str) -> bool {
        let removed = self.entries_mut().remove(name).is_some();
        self.set_enabled(name, true);
        if let Some(Value::Object(servers)) = self.oxide_mut().get_mut("servers") {
            servers.remove(name);
        }
        removed
    }

    pub fn set_enabled(&mut self, name: &str, enabled: bool) {
        let mut disabled = self.disabled();
        disabled.retain(|d| d != name);
        if !enabled {
            disabled.push(name.to_owned());
        }
        let oxide = self.oxide_mut();
        if disabled.is_empty() {
            oxide.remove("disabledServers");
        } else {
            oxide.insert("disabledServers".into(), json!(disabled));
        }
    }

    /// A pre-registered OAuth client for a server (`oxide.servers.<name>.oauthClientId`).
    pub fn oauth_client(&self, name: &str) -> Option<(String, Option<String>)> {
        let s = self.root.get("oxide")?.get("servers")?.get(name)?;
        let id = s.get("oauthClientId")?.as_str()?.to_owned();
        let secret = s
            .get("oauthClientSecret")
            .and_then(Value::as_str)
            .map(str::to_owned);
        Some((id, secret))
    }
}

/// An installed desktop extension's entry (`oxide.extensions.<name>`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ExtensionEntry {
    pub enabled: bool,
    pub version: String,
    pub installed_at: Timestamp,
    /// Settings that aren't sensitive (those are in the keychain).
    pub values: BTreeMap<String, ExtensionValue>,
    /// Sensitive settings that have a value in the keychain.
    pub secrets: Vec<String>,
}

impl Default for ExtensionEntry {
    fn default() -> Self {
        Self {
            enabled: true,
            version: String::new(),
            installed_at: 0,
            values: BTreeMap::new(),
            secrets: Vec::new(),
        }
    }
}

impl OxideConfig {
    fn extensions_map(&self) -> Option<&Map<String, Value>> {
        self.root
            .get("oxide")
            .and_then(|o| o.get("extensions"))
            .and_then(Value::as_object)
    }

    /// Installed extensions (entries that can't be read are skipped).
    pub fn extensions(&self) -> Vec<(String, ExtensionEntry)> {
        self.extensions_map()
            .map(|m| {
                m.iter()
                    .filter_map(|(k, v)| Some((k.clone(), serde_json::from_value(v.clone()).ok()?)))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn extension(&self, name: &str) -> Option<ExtensionEntry> {
        serde_json::from_value(self.extensions_map()?.get(name)?.clone()).ok()
    }

    pub fn is_extension(&self, name: &str) -> bool {
        self.extensions_map().is_some_and(|m| m.contains_key(name))
    }

    pub fn set_extension(&mut self, name: &str, entry: &ExtensionEntry) {
        let value = serde_json::to_value(entry).unwrap_or(Value::Null);
        object_at(self.oxide_mut(), "extensions").insert(name.to_owned(), value);
    }

    pub fn remove_extension(&mut self, name: &str) -> bool {
        let oxide = self.oxide_mut();
        let Some(Value::Object(m)) = oxide.get_mut("extensions") else {
            return false;
        };
        let removed = m.remove(name).is_some();
        if m.is_empty() {
            oxide.remove("extensions");
        }
        removed
    }

    /// Whether `name` is a server entry in `mcpServers`.
    pub fn has_server(&self, name: &str) -> bool {
        self.entries().is_some_and(|m| m.contains_key(name))
    }
}

/// The object under `key`, created (or replacing a non-object) if needed.
fn object_at<'a>(root: &'a mut Map<String, Value>, key: &str) -> &'a mut Map<String, Value> {
    let v = root.entry(key).or_insert_with(|| Value::Object(Map::new()));
    if !v.is_object() {
        *v = Value::Object(Map::new());
    }
    v.as_object_mut()
        .expect("the value was just made an object")
}

fn validate(cfg: &McpServerConfig) -> Result<String, McpError> {
    let name = cfg.name.trim().to_owned();
    if name.is_empty() {
        return Err(McpError::Config("give the server a name".into()));
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(McpError::Config(format!(
            "server names can be at most {MAX_NAME_CHARS} characters"
        )));
    }
    if name.contains('/') {
        return Err(McpError::Config("server names can't contain \"/\"".into()));
    }
    match cfg.transport {
        McpTransport::Stdio if cfg.command.trim().is_empty() => Err(McpError::Config(
            "enter the command that starts the server".into(),
        )),
        McpTransport::Http => {
            let url = url::Url::parse(cfg.url.trim())
                .map_err(|_| McpError::Config("enter a valid URL".into()))?;
            if matches!(url.scheme(), "http" | "https") {
                Ok(name)
            } else {
                Err(McpError::Config(
                    "the URL must start with http:// or https://".into(),
                ))
            }
        }
        McpTransport::Stdio => Ok(name),
    }
}

fn string_map(v: Option<&Value>) -> BTreeMap<String, String> {
    v.and_then(Value::as_object)
        .map(|m| {
            m.iter()
                .map(|(k, v)| {
                    let v = match v {
                        Value::String(s) => s.clone(),
                        Value::Null => String::new(),
                        other => other.to_string(),
                    };
                    (k.clone(), v)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Reads one `mcpServers` entry.
pub fn entry_to_config(name: &str, entry: &Value) -> Result<McpServerConfig, String> {
    let Value::Object(e) = entry else {
        return Err("the entry must be an object".into());
    };
    let ty = e.get("type").and_then(Value::as_str).unwrap_or("");
    let http = e.contains_key("url")
        || matches!(
            ty,
            "http" | "streamable-http" | "streamableHttp" | "streamable_http" | "sse"
        );
    if http {
        let url = e
            .get("url")
            .and_then(Value::as_str)
            .ok_or("a remote server needs a \"url\"")?;
        return Ok(McpServerConfig {
            name: name.to_owned(),
            transport: McpTransport::Http,
            command: String::new(),
            args: Vec::new(),
            env: BTreeMap::new(),
            url: url.to_owned(),
            headers: string_map(e.get("headers")),
            enabled: true,
        });
    }
    let command = e
        .get("command")
        .and_then(Value::as_str)
        .ok_or("a local server needs a \"command\"")?;
    let args = match e.get("args") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(a)) => a
            .iter()
            .map(|v| match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            })
            .collect(),
        Some(_) => return Err("\"args\" must be a list".into()),
    };
    Ok(McpServerConfig {
        name: name.to_owned(),
        transport: McpTransport::Stdio,
        command: command.to_owned(),
        args,
        env: string_map(e.get("env")),
        url: String::new(),
        headers: BTreeMap::new(),
        enabled: true,
    })
}

/// Writes one entry, keeping keys Oxide doesn't manage from `existing`.
pub fn config_to_entry(cfg: &McpServerConfig, existing: Option<&Value>) -> Value {
    let mut e: Map<String, Value> = existing
        .and_then(Value::as_object)
        .map(|m| {
            m.iter()
                .filter(|(k, _)| !MANAGED_KEYS.contains(&k.as_str()))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        })
        .unwrap_or_default();
    match cfg.transport {
        McpTransport::Stdio => {
            e.insert("command".into(), json!(cfg.command.trim()));
            e.insert("args".into(), json!(cfg.args));
            if !cfg.env.is_empty() {
                e.insert("env".into(), json!(cfg.env));
            }
        }
        McpTransport::Http => {
            e.insert("type".into(), json!("http"));
            e.insert("url".into(), json!(cfg.url.trim()));
            if !cfg.headers.is_empty() {
                e.insert("headers".into(), json!(cfg.headers));
            }
        }
    }
    Value::Object(e)
}

/// Where Claude desktop keeps its config on this OS (first existing one wins).
pub fn claude_desktop_paths() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if cfg!(windows) {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            out.push(
                PathBuf::from(appdata)
                    .join("Claude")
                    .join("claude_desktop_config.json"),
            );
        }
        // The Microsoft Store build keeps its data in a package folder.
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            let packages = PathBuf::from(local).join("Packages");
            if let Ok(dirs) = std::fs::read_dir(&packages) {
                for d in dirs.flatten() {
                    let name = d.file_name().to_string_lossy().into_owned();
                    if name.starts_with("Claude_") {
                        out.push(
                            d.path()
                                .join("LocalCache")
                                .join("Roaming")
                                .join("Claude")
                                .join("claude_desktop_config.json"),
                        );
                    }
                }
            }
        }
    } else if let Some(home) = crate::process::home_dir() {
        if cfg!(target_os = "macos") {
            out.push(
                home.join("Library")
                    .join("Application Support")
                    .join("Claude")
                    .join("claude_desktop_config.json"),
            );
        } else {
            let config = std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".config"));
            out.push(config.join("Claude").join("claude_desktop_config.json"));
        }
    }
    out
}

/// Compares Claude desktop's servers (in `theirs`, the file's text) with ours.
pub fn import_preview(ours: &OxideConfig, path: &Path, theirs: Option<&str>) -> ImportPreview {
    let mut candidates = Vec::new();
    if let Some(text) = theirs
        && let Ok(cfg) = OxideConfig::parse(text)
    {
        for (name, entry) in cfg.servers() {
            let Ok(mut config) = entry else { continue };
            config.enabled = true;
            let status = match ours.server(&name) {
                None => ImportStatus::New,
                Some(Ok(mut existing)) => {
                    existing.enabled = true;
                    if existing == config {
                        ImportStatus::Same
                    } else {
                        ImportStatus::Changed
                    }
                }
                Some(Err(_)) => ImportStatus::Changed,
            };
            candidates.push(ImportCandidate { config, status });
        }
    }
    ImportPreview {
        path: path.display().to_string(),
        found: theirs.is_some(),
        candidates,
    }
}

/// Copies the named servers from Claude desktop's config text into `ours`. Returns how many.
pub fn import_into(
    ours: &mut OxideConfig,
    theirs: &str,
    names: &[String],
) -> Result<usize, McpError> {
    let cfg = OxideConfig::parse(theirs)?;
    let Some(entries) = cfg.entries() else {
        return Ok(0);
    };
    let mut n = 0;
    for name in names {
        if let Some(entry) = entries.get(name) {
            // Paste the entry as is (it's the same schema), keeping its extra keys.
            ours.entries_mut().insert(name.clone(), entry.clone());
            ours.set_enabled(name, true);
            n += 1;
        }
    }
    Ok(n)
}

/// The config file on disk, with atomic writes and change polling.
pub struct ConfigFile {
    path: PathBuf,
    /// The text Oxide last wrote or read, to tell our own writes from the user's edits.
    last: Mutex<Option<String>>,
}

impl ConfigFile {
    pub fn new(path: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            path,
            last: Mutex::new(None),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn remember(&self, text: &str) {
        *self.last.lock().unwrap_or_else(|e| e.into_inner()) = Some(text.to_owned());
    }

    /// The file's text, creating it (with an empty `mcpServers`) if it doesn't exist yet.
    pub async fn read_text(&self) -> Result<String, McpError> {
        match tokio::fs::read_to_string(&self.path).await {
            Ok(t) => Ok(t),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let text = OxideConfig::default().to_text();
                self.write_text_unchecked(&text).await?;
                Ok(text)
            }
            Err(e) => Err(McpError::Config(format!(
                "couldn't read {}: {e}",
                self.path.display()
            ))),
        }
    }

    pub async fn load(&self) -> Result<OxideConfig, McpError> {
        let text = self.read_text().await?;
        self.remember(&text);
        OxideConfig::parse(&text)
    }

    pub async fn save(&self, cfg: &OxideConfig) -> Result<(), McpError> {
        self.write_text_unchecked(&cfg.to_text()).await
    }

    /// Saves text from the editor after checking that it parses.
    pub async fn save_text(&self, text: &str) -> Result<OxideConfig, McpError> {
        let cfg = OxideConfig::parse(text)?;
        self.write_text_unchecked(text).await?;
        Ok(cfg)
    }

    async fn write_text_unchecked(&self, text: &str) -> Result<(), McpError> {
        let err = |e: std::io::Error| {
            McpError::Config(format!("couldn't save {}: {e}", self.path.display()))
        };
        if let Some(dir) = self.path.parent() {
            tokio::fs::create_dir_all(dir).await.map_err(err)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        tokio::fs::write(&tmp, text).await.map_err(err)?;
        tokio::fs::rename(&tmp, &self.path).await.map_err(err)?;
        self.remember(text);
        Ok(())
    }

    pub async fn file(&self) -> Result<McpConfigFile, McpError> {
        Ok(McpConfigFile {
            path: self.path.display().to_string(),
            text: self.read_text().await?,
        })
    }

    /// Polls the file every `every` and calls `on_change` with the new config (or the parse
    /// error) whenever its text changes to something Oxide didn't write itself.
    pub fn watch<F>(self: &Arc<Self>, every: Duration, on_change: F) -> tokio::task::JoinHandle<()>
    where
        F: Fn(Result<OxideConfig, McpError>) + Send + Sync + 'static,
    {
        let me = self.clone();
        tokio::spawn(async move {
            let mut stamp: Option<(SystemTime, u64)> = None;
            loop {
                tokio::time::sleep(every).await;
                let Ok(meta) = tokio::fs::metadata(&me.path).await else {
                    continue;
                };
                let now = (
                    meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                    meta.len(),
                );
                if stamp == Some(now) {
                    continue;
                }
                let first = stamp.is_none();
                stamp = Some(now);
                let Ok(text) = tokio::fs::read_to_string(&me.path).await else {
                    continue;
                };
                let ours = me.last.lock().unwrap_or_else(|e| e.into_inner()).as_deref()
                    == Some(text.as_str());
                if ours || (first && me.last.lock().unwrap_or_else(|e| e.into_inner()).is_none()) {
                    // Our own write, or the first look before anything was loaded.
                    me.remember(&text);
                    continue;
                }
                me.remember(&text);
                on_change(OxideConfig::parse(&text));
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stdio(name: &str, command: &str) -> McpServerConfig {
        McpServerConfig {
            name: name.into(),
            transport: McpTransport::Stdio,
            command: command.into(),
            args: vec!["-y".into(), "pkg".into()],
            env: BTreeMap::new(),
            url: String::new(),
            headers: BTreeMap::new(),
            enabled: true,
        }
    }

    #[test]
    fn reads_claude_desktop_entries() {
        let cfg = OxideConfig::parse(
            r#"{"mcpServers": {
                "fs": {"command": "npx", "args": ["-y", "@modelcontextprotocol/server-filesystem", "C:\\x"], "env": {"A": "1", "N": 2}},
                "remote": {"type": "http", "url": "https://x/mcp", "headers": {"K": "v"}},
                "bad": {"args": []}
              },
              "globalShortcut": "Ctrl+Space"}"#,
        )
        .expect("parses");
        let servers = cfg.servers();
        assert_eq!(servers.len(), 3);
        let fs = cfg.server("fs").expect("fs").expect("valid");
        assert_eq!(fs.transport, McpTransport::Stdio);
        assert_eq!(fs.args[2], "C:\\x");
        assert_eq!(fs.env.get("N").map(String::as_str), Some("2"));
        let remote = cfg.server("remote").expect("remote").expect("valid");
        assert_eq!(remote.transport, McpTransport::Http);
        assert_eq!(remote.headers.get("K").map(String::as_str), Some("v"));
        assert!(cfg.server("bad").expect("bad").is_err());
        // Unknown top-level keys survive a rewrite.
        assert!(cfg.to_text().contains("globalShortcut"));
    }

    #[test]
    fn upsert_rename_disable_and_remove() {
        let mut cfg = OxideConfig::parse(
            r#"{"mcpServers": {"a": {"command": "old", "custom": true}},
                "oxide": {"servers": {"a": {"oauthClientId": "cid"}}}}"#,
        )
        .expect("parses");
        let mut a = stdio("b", "npx");
        a.enabled = false;
        cfg.upsert(&a, Some("a")).expect("renames");
        assert!(cfg.server("a").is_none());
        let b = cfg.server("b").expect("b").expect("valid");
        assert_eq!(b.command, "npx");
        assert!(!b.enabled);
        assert_eq!(cfg.oauth_client("b"), Some(("cid".into(), None)));
        // Keys Oxide doesn't manage stay in the entry; Oxide-only keys stay out of it.
        let text = cfg.to_text();
        assert!(text.contains("\"custom\": true"));
        let v: Value = serde_json::from_str(&text).expect("json");
        assert!(v["mcpServers"]["b"].get("enabled").is_none());
        assert_eq!(v["oxide"]["disabledServers"], json!(["b"]));

        cfg.set_enabled("b", true);
        assert!(cfg.server("b").expect("b").expect("valid").enabled);
        assert!(cfg.to_text().find("disabledServers").is_none());

        cfg.upsert(&stdio("c", "uvx"), None).expect("adds");
        assert!(matches!(
            cfg.upsert(&stdio("c", "x"), Some("b")),
            Err(McpError::Config(_))
        ));
        assert!(cfg.remove("c"));
        assert!(!cfg.remove("c"));
    }

    #[test]
    fn validation_messages() {
        let mut cfg = OxideConfig::default();
        assert!(cfg.upsert(&stdio("", "npx"), None).is_err());
        assert!(cfg.upsert(&stdio("a/b", "npx"), None).is_err());
        assert!(cfg.upsert(&stdio("a", " "), None).is_err());
        let mut h = stdio("h", "");
        h.transport = McpTransport::Http;
        h.url = "ftp://x".into();
        assert!(cfg.upsert(&h, None).is_err());
        h.url = "https://x.example/mcp".into();
        cfg.upsert(&h, None).expect("http ok");
        let v: Value = serde_json::from_str(&cfg.to_text()).expect("json");
        assert_eq!(
            v["mcpServers"]["h"],
            json!({"type": "http", "url": "https://x.example/mcp"})
        );
    }

    #[test]
    fn bad_json_explains_where() {
        let err = OxideConfig::parse("{\n  \"mcpServers\": {,}\n}").expect_err("bad");
        assert!(err.to_string().contains("line 2"), "{err}");
        assert!(OxideConfig::parse("[]").is_err());
        assert!(OxideConfig::parse("").is_ok());
    }

    #[test]
    fn import_compares_and_copies() {
        let mut ours = OxideConfig::default();
        ours.upsert(&stdio("same", "npx"), None).expect("adds");
        ours.upsert(&stdio("changed", "npx"), None).expect("adds");
        ours.set_enabled("same", false);
        let theirs = r#"{"mcpServers": {
            "same": {"command": "npx", "args": ["-y", "pkg"]},
            "changed": {"command": "uvx", "args": []},
            "new": {"command": "node", "args": ["s.js"], "extra": 1}
        }}"#;
        let p = import_preview(&ours, Path::new("x.json"), Some(theirs));
        let status = |n: &str| {
            p.candidates
                .iter()
                .find(|c| c.config.name == n)
                .map(|c| c.status)
        };
        assert_eq!(status("same"), Some(ImportStatus::Same));
        assert_eq!(status("changed"), Some(ImportStatus::Changed));
        assert_eq!(status("new"), Some(ImportStatus::New));
        let n = import_into(&mut ours, theirs, &["new".into(), "changed".into()]).expect("imports");
        assert_eq!(n, 2);
        assert_eq!(
            ours.server("changed").expect("c").expect("ok").command,
            "uvx"
        );
        assert!(ours.to_text().contains("\"extra\": 1"));
        assert!(!import_preview(&ours, Path::new("x"), None).found);
    }

    #[test]
    fn extension_entries_live_under_oxide() {
        let mut cfg = OxideConfig::default();
        let mut e = ExtensionEntry {
            version: "1.0.0".into(),
            ..Default::default()
        };
        e.values.insert("depth".into(), ExtensionValue::Number(3.0));
        e.secrets.push("key".into());
        cfg.set_extension("files", &e);
        assert!(cfg.is_extension("files"));
        assert!(!cfg.has_server("files"));
        assert_eq!(cfg.extension("files"), Some(e.clone()));
        let v: Value = serde_json::from_str(&cfg.to_text()).expect("json");
        assert_eq!(v["oxide"]["extensions"]["files"]["version"], json!("1.0.0"));
        assert_eq!(
            v["oxide"]["extensions"]["files"]["values"]["depth"],
            json!(3.0)
        );
        assert!(v["mcpServers"].as_object().is_some_and(|m| m.is_empty()));
        // Round trip through the text.
        let again = OxideConfig::parse(&cfg.to_text()).expect("parses");
        assert_eq!(again.extensions(), vec![("files".to_string(), e)]);
        assert!(cfg.remove_extension("files"));
        assert!(!cfg.remove_extension("files"));
        assert!(!cfg.to_text().contains("extensions"));
    }

    #[tokio::test]
    async fn file_round_trip_and_watch() {
        let dir = tempfile::tempdir().expect("tmp");
        let file = ConfigFile::new(dir.path().join("sub").join(CONFIG_FILE_NAME));
        let cfg = file.load().await.expect("creates");
        assert_eq!(cfg.names().len(), 0);
        assert!(file.path().exists());

        let seen = Arc::new(Mutex::new(Vec::<Result<Vec<String>, String>>::new()));
        let s = seen.clone();
        let task = file.watch(Duration::from_millis(20), move |c| {
            s.lock()
                .expect("lock")
                .push(c.map(|c| c.names()).map_err(|e| e.to_string()));
        });
        tokio::time::sleep(Duration::from_millis(60)).await;
        // Our own writes don't trigger the watcher.
        let mut cfg = OxideConfig::default();
        cfg.upsert(&stdio("mine", "npx"), None).expect("adds");
        file.save(&cfg).await.expect("saves");
        tokio::time::sleep(Duration::from_millis(80)).await;
        assert!(seen.lock().expect("lock").is_empty());
        // The user's edits do, including broken ones.
        std::fs::write(
            file.path(),
            r#"{"mcpServers": {"theirs": {"command": "x"}}}"#,
        )
        .expect("write");
        tokio::time::sleep(Duration::from_millis(120)).await;
        std::fs::write(file.path(), "{ broken").expect("write");
        tokio::time::sleep(Duration::from_millis(120)).await;
        task.abort();
        let seen = seen.lock().expect("lock").clone();
        assert_eq!(seen.first(), Some(&Ok(vec!["theirs".to_string()])));
        assert!(seen.last().is_some_and(Result::is_err));
        assert!(file.save_text("nope").await.is_err());
    }
}
