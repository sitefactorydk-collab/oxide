//! Which local server commands the user has trusted (Phase 18).
//!
//! A stdio server runs a program with the user's rights, and `oxide_config.json` can be
//! edited by anything on the computer. So a local server whose command line Oxide hasn't run
//! before — added by hand or by another program, or changed since — waits in
//! [`McpStatus::NeedsTrust`](oxide_core::McpStatus::NeedsTrust) until the user trusts it.
//! Servers added or edited in Oxide's own Settings, imported from Claude desktop after the
//! confirmation, and desktop extensions (which have their own install dialog) are trusted
//! as they are saved.
//!
//! What is remembered is a SHA-256 fingerprint of the command, its arguments and its
//! environment, per server name, in `<data>/mcp-trust.json` — outside the config file, so
//! editing the config can't also mark it trusted. On the first start with this file missing
//! (an upgrade from an older Oxide), the servers already configured are trusted as they are.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

use oxide_core::{McpServerConfig, McpTransport};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// How a server's current command compares with what the user trusted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustState {
    Trusted,
    /// Never trusted under this name.
    New,
    /// Another command line was trusted under this name.
    Changed,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct TrustFile {
    version: u32,
    /// Server name → fingerprint of the trusted command line.
    servers: BTreeMap<String, String>,
}

/// The fingerprint of a local server's command line (command, arguments, environment).
pub fn fingerprint(config: &McpServerConfig) -> String {
    let mut h = Sha256::new();
    let mut put = |s: &str| {
        h.update((s.len() as u64).to_le_bytes());
        h.update(s.as_bytes());
    };
    put("oxide-mcp-trust/1");
    put(&config.command);
    for a in &config.args {
        put(a);
    }
    put("--env--");
    for (k, v) in &config.env {
        put(k);
        put(v);
    }
    hex_lower(&h.finalize())
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 15) as usize] as char);
    }
    s
}

/// Remembered trust decisions. With no file (tests, `ManagerOptions::trust_file: None`),
/// every command is trusted.
pub struct TrustStore {
    path: Option<PathBuf>,
    servers: Mutex<BTreeMap<String, String>>,
    /// The file didn't exist: trust what is configured at the first apply.
    seed: Mutex<bool>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl TrustStore {
    /// Reads the file (a small JSON document; read once at start).
    pub fn open(path: Option<PathBuf>) -> Self {
        let mut seed = false;
        let mut servers = BTreeMap::new();
        if let Some(p) = &path {
            match std::fs::read_to_string(p) {
                Ok(text) => match serde_json::from_str::<TrustFile>(&text) {
                    Ok(f) => servers = f.servers,
                    Err(e) => {
                        // A damaged file trusts nothing: every server asks again.
                        tracing::warn!(
                            "MCP trust file can't be read ({e}); servers will ask again"
                        );
                    }
                },
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => seed = true,
                Err(e) => tracing::warn!("MCP trust file can't be read: {e}"),
            }
        }
        Self {
            path,
            servers: Mutex::new(servers),
            seed: Mutex::new(seed),
        }
    }

    pub fn enabled(&self) -> bool {
        self.path.is_some()
    }

    /// Whether `config` (of server `name`) may start without asking.
    pub fn state(&self, name: &str, config: &McpServerConfig) -> TrustState {
        if self.path.is_none() || config.transport != McpTransport::Stdio {
            return TrustState::Trusted;
        }
        match lock(&self.servers).get(name) {
            Some(fp) if *fp == fingerprint(config) => TrustState::Trusted,
            Some(_) => TrustState::Changed,
            None => TrustState::New,
        }
    }

    /// Trusts these servers' current command lines. Returns whether anything changed.
    pub fn trust<'a>(
        &self,
        configs: impl IntoIterator<Item = (&'a str, &'a McpServerConfig)>,
    ) -> bool {
        if self.path.is_none() {
            return false;
        }
        let mut servers = lock(&self.servers);
        let mut changed = false;
        for (name, config) in configs {
            if config.transport != McpTransport::Stdio {
                continue;
            }
            let fp = fingerprint(config);
            if servers.get(name) != Some(&fp) {
                servers.insert(name.to_owned(), fp);
                changed = true;
            }
        }
        changed
    }

    /// Forgets a removed server, so a new one under its name asks.
    pub fn forget(&self, name: &str) -> bool {
        self.path.is_some() && lock(&self.servers).remove(name).is_some()
    }

    /// `true` once, when the file was missing at start (see the module docs).
    pub fn take_seed(&self) -> bool {
        std::mem::take(&mut *lock(&self.seed))
    }

    /// Writes the file (in the background; the newest state wins).
    pub fn save_soon(&self) {
        let Some(path) = self.path.clone() else {
            return;
        };
        let file = TrustFile {
            version: 1,
            servers: lock(&self.servers).clone(),
        };
        let Ok(text) = serde_json::to_string_pretty(&file) else {
            return;
        };
        let write = move || {
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let tmp = path.with_extension("json.partial");
            if let Err(e) = std::fs::write(&tmp, text).and_then(|()| std::fs::rename(&tmp, &path)) {
                tracing::warn!("couldn't save the MCP trust file: {e}");
            }
        };
        match tokio::runtime::Handle::try_current() {
            Ok(rt) => {
                // One writer at a time keeps the newest state last.
                static WRITE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
                rt.spawn(async move {
                    let _guard = WRITE.lock().await;
                    let _ = tokio::task::spawn_blocking(write).await;
                });
            }
            Err(_) => write(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stdio(cmd: &str, args: &[&str]) -> McpServerConfig {
        McpServerConfig {
            name: "x".into(),
            transport: McpTransport::Stdio,
            command: cmd.into(),
            args: args.iter().map(|s| s.to_string()).collect(),
            env: BTreeMap::new(),
            url: String::new(),
            headers: BTreeMap::new(),
            enabled: true,
        }
    }

    #[test]
    fn fingerprints_cover_command_args_and_env() {
        let a = stdio("npx", &["-y", "server"]);
        let mut b = a.clone();
        b.args = vec!["-y server".into()];
        assert_ne!(
            fingerprint(&a),
            fingerprint(&b),
            "argument boundaries count"
        );
        let mut c = a.clone();
        c.env.insert("NODE_OPTIONS".into(), "--require x.js".into());
        assert_ne!(fingerprint(&a), fingerprint(&c));
        let mut d = a.clone();
        d.enabled = false;
        d.name = "other".into();
        assert_eq!(fingerprint(&a), fingerprint(&d), "name and switch don't");
        assert_eq!(fingerprint(&a).len(), 64);
    }

    #[test]
    fn new_changed_trusted_and_saved() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("mcp-trust.json");
        let store = TrustStore::open(Some(path.clone()));
        assert!(store.take_seed());
        assert!(!store.take_seed());
        let a = stdio("npx", &["a"]);
        assert_eq!(store.state("a", &a), TrustState::New);
        assert!(store.trust([("a", &a)]));
        assert!(!store.trust([("a", &a)]));
        assert_eq!(store.state("a", &a), TrustState::Trusted);
        assert_eq!(store.state("a", &stdio("npx", &["b"])), TrustState::Changed);
        let mut http = a.clone();
        http.transport = McpTransport::Http;
        assert_eq!(store.state("zzz", &http), TrustState::Trusted);
        store.save_soon();
        let again = TrustStore::open(Some(path));
        assert!(!again.take_seed());
        assert_eq!(again.state("a", &a), TrustState::Trusted);
        assert!(again.forget("a"));
        assert_eq!(again.state("a", &a), TrustState::New);
    }

    #[test]
    fn a_damaged_file_trusts_nothing_and_no_file_trusts_all() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("mcp-trust.json");
        std::fs::write(&path, "{not json").expect("write");
        let store = TrustStore::open(Some(path));
        assert!(!store.take_seed());
        assert_eq!(store.state("a", &stdio("x", &[])), TrustState::New);
        let none = TrustStore::open(None);
        assert_eq!(none.state("a", &stdio("x", &[])), TrustState::Trusted);
    }
}
