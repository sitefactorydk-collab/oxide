//! Localhost WebSocket bridge between Oxide and its Chrome extension, with token pairing.
//!
//! - Listens on `127.0.0.1` only, on a port from Settings → Chrome.
//! - Accepts a WebSocket upgrade only from a `chrome-extension://<id>` origin and only with a
//!   `Host` of this computer (so a web page, or a DNS-rebinding site, can't connect).
//! - The first message must be `pair` (with the six-word code Oxide shows) or `hello` (with
//!   the token a pairing gave). Anything else, or nothing within 10 seconds, closes the
//!   connection. Tokens are 256 random bits kept in the keychain and bound to the extension's
//!   id.
//! - Messages are JSON-RPC 2.0. Oxide sends `tools/call` and `session/release`; the extension
//!   sends `ping` (its heartbeat, which also keeps its service worker awake) and
//!   `session/stopped` (the user pressed Stop in a tab). Oxide pings every 20 seconds and
//!   drops a connection silent for 60.
//!
//! The protocol is described for the extension's side in `extension/README.md`.

pub mod words;
pub mod ws;

#[cfg(any(test, feature = "testing"))]
pub mod testing;

#[cfg(test)]
mod tests;

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use base64::Engine as _;
use oxide_core::chrome::{ChromeSettings, ChromeStatus, PairedBrowser, PairingCode};
use oxide_core::{McpContent, McpToolResult};
use oxide_store::{Secret, SecretStore};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::io::{AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinHandle;

use ws::Message;

/// Version of the bridge protocol. The extension sends it in `pair` and `hello`.
pub const PROTOCOL: u32 = 1;

/// How long a new connection has to say `pair` or `hello` (shorter in unit tests).
pub const AUTH_TIMEOUT: Duration = if cfg!(test) {
    Duration::from_millis(400)
} else {
    Duration::from_secs(10)
};
/// How often Oxide pings a connected extension.
pub const HEARTBEAT: Duration = if cfg!(test) {
    Duration::from_millis(100)
} else {
    Duration::from_secs(20)
};
/// A connection that sends nothing for this long is dropped.
pub const IDLE_TIMEOUT: Duration = if cfg!(test) {
    Duration::from_millis(500)
} else {
    Duration::from_secs(60)
};
/// The longest one tool call may take (the extension may wait for the user to allow a site).
pub const CALL_TIMEOUT: Duration = Duration::from_secs(180);
/// How long a pairing code is valid.
pub const PAIRING_TTL: Duration = Duration::from_secs(5 * 60);
/// Wrong codes before a pairing code is withdrawn.
pub const PAIRING_TRIES: u32 = 5;
/// Largest message the extension may send (screenshots are a few MB of base64).
pub const MAX_MESSAGE: usize = 32 * 1024 * 1024;

/// JSON-RPC error codes the bridge answers with.
pub mod codes {
    pub const PARSE: i64 = -32700;
    pub const INVALID: i64 = -32600;
    pub const NO_METHOD: i64 = -32601;
    pub const UNAUTHORIZED: i64 = -32001;
    pub const NO_PAIRING: i64 = -32002;
    pub const WRONG_CODE: i64 = -32003;
    pub const PROTOCOL: i64 = -32004;
}

/// WebSocket close codes the bridge uses (4000–4999 are for applications).
pub mod close {
    pub const NORMAL: u16 = 1000;
    pub const GOING_AWAY: u16 = 1001;
    pub const PROTOCOL_ERROR: u16 = 1002;
    pub const POLICY: u16 = 1008;
    pub const TOO_BIG: u16 = 1009;
    pub const UNAUTHORIZED: u16 = 4001;
    /// The same browser connected again; the older connection is closed.
    pub const REPLACED: u16 = 4002;
    /// The user removed the pairing in Oxide.
    pub const FORGOTTEN: u16 = 4003;
}

#[derive(Debug, thiserror::Error)]
pub enum BridgeError {
    #[error(
        "Chrome isn't connected to Oxide. Open Chrome with the Oxide extension and check that \
         its popup says “Connected” (Settings → Chrome in Oxide shows how to pair it)."
    )]
    NotConnected,
    #[error("Chrome disconnected before the action finished.")]
    Disconnected,
    #[error("Chrome didn't answer within {} seconds.", CALL_TIMEOUT.as_secs())]
    Timeout,
    #[error("The Chrome extension refused the call: {0}")]
    Remote(String),
    #[error("The Chrome extension sent an answer Oxide couldn't read: {0}")]
    BadAnswer(String),
}

/// Where the bridge reports changes.
pub trait BridgeSink: Send + Sync + 'static {
    /// The status changed (connections, pairing, listening).
    fn changed(&self, status: &ChromeStatus);
    /// The user pressed Stop in a tab the chat `session` was working in.
    fn stopped(&self, session: &str);
}

/// A sink that drops everything (tests, headless use).
pub struct NoSink;

impl BridgeSink for NoSink {
    fn changed(&self, _: &ChromeStatus) {}
    fn stopped(&self, _: &str) {}
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn token_key(browser_id: &str) -> String {
    format!("chrome-token-{browser_id}")
}

/// A new pairing token: 256 random bits, base64url.
fn new_token() -> String {
    let mut b = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut b);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b)
}

/// Compares secrets in time independent of where they differ.
fn same_secret(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// The extension id of a `chrome-extension://<id>` origin: 32 letters a–p.
pub fn extension_id(origin: &str) -> Option<&str> {
    let id = origin.trim().strip_prefix("chrome-extension://")?;
    let id = id.strip_suffix('/').unwrap_or(id);
    (id.len() == 32 && id.bytes().all(|b| (b'a'..=b'p').contains(&b))).then_some(id)
}

/// Whether a `Host` header names this computer on `port`.
fn local_host(host: &str, port: u16) -> bool {
    let host = host.trim().to_ascii_lowercase();
    ["127.0.0.1", "localhost", "[::1]"]
        .iter()
        .any(|h| host == format!("{h}:{port}"))
}

/// Checks an upgrade request: `Ok((key, extension id))`, or the HTTP refusal to send.
fn check_upgrade(head: &ws::RequestHead, port: u16) -> Result<(String, String), String> {
    if head.method != "GET" || !head.header_has("upgrade", "websocket") {
        return Err(ws::refusal(
            400,
            "Bad Request",
            "Oxide's Chrome bridge only speaks WebSocket.",
        ));
    }
    if !head.header("host").is_some_and(|h| local_host(h, port)) {
        return Err(ws::refusal(403, "Forbidden", "Wrong host."));
    }
    let Some(ext) = head.header("origin").and_then(extension_id) else {
        return Err(ws::refusal(
            403,
            "Forbidden",
            "Only Oxide's Chrome extension may connect.",
        ));
    };
    if head.header("sec-websocket-version") != Some("13") {
        return Err(ws::refusal(
            426,
            "Upgrade Required",
            "WebSocket version 13 only.",
        ));
    }
    let Some(key) = head.header("sec-websocket-key").filter(|k| {
        base64::engine::general_purpose::STANDARD
            .decode(k.trim())
            .is_ok_and(|b| b.len() == 16)
    }) else {
        return Err(ws::refusal(400, "Bad Request", "Missing WebSocket key."));
    };
    Ok((key.trim().to_owned(), ext.to_owned()))
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct PairingsFile {
    paired: Vec<PairedBrowser>,
}

struct Pending {
    code: String,
    expires_at: Instant,
    expires_at_ms: i64,
    tries_left: u32,
}

enum Outgoing {
    Call {
        id: u64,
        message: String,
        reply: oneshot::Sender<Result<Value, String>>,
    },
    Notify(String),
    Close(u16, String),
}

struct Conn {
    conn_id: u64,
    browser_id: String,
    name: String,
    version: String,
    tx: mpsc::Sender<Outgoing>,
}

struct Listening {
    port: u16,
    shutdown: watch::Sender<bool>,
    task: JoinHandle<()>,
}

#[derive(Default)]
struct State {
    settings: ChromeSettings,
    listening: Option<Listening>,
    error: Option<String>,
    paired: Vec<PairedBrowser>,
    pairing: Option<Pending>,
    /// Authenticated connections; the last is the one tools go to.
    conns: Vec<Conn>,
    sessions: BTreeSet<String>,
    /// Chats the user stopped in Chrome; the extension refuses them until they are released.
    stopped: BTreeSet<String>,
    extension_dir: Option<String>,
}

struct Inner {
    secrets: Arc<dyn SecretStore>,
    file: PathBuf,
    sink: Arc<dyn BridgeSink>,
    state: Mutex<State>,
    next_id: AtomicU64,
}

/// The bridge. Cheap to clone.
#[derive(Clone)]
pub struct Bridge {
    inner: Arc<Inner>,
}

impl Bridge {
    /// A bridge that keeps its list of paired browsers in `file` (JSON) and their tokens in
    /// `secrets`. It doesn't listen until [`Bridge::configure`] turns it on.
    pub async fn open(
        file: PathBuf,
        secrets: Arc<dyn SecretStore>,
        sink: Arc<dyn BridgeSink>,
    ) -> Self {
        let paired = match tokio::fs::read(&file).await {
            Ok(b) => serde_json::from_slice::<PairingsFile>(&b)
                .map(|f| f.paired)
                .unwrap_or_else(|e| {
                    tracing::warn!("couldn't read {}: {e}", file.display());
                    Vec::new()
                }),
            Err(_) => Vec::new(),
        };
        Self {
            inner: Arc::new(Inner {
                secrets,
                file,
                sink,
                state: Mutex::new(State {
                    paired,
                    ..State::default()
                }),
                next_id: AtomicU64::new(1),
            }),
        }
    }

    /// The folder users load unpacked in `chrome://extensions` (shown in Settings).
    pub fn set_extension_dir(&self, dir: Option<String>) {
        lock(&self.inner.state).extension_dir = dir;
    }

    /// Applies Settings → Chrome: starts, moves or stops the listener.
    pub async fn configure(&self, settings: &ChromeSettings) {
        let settings = settings.clone().normalized();
        let (old, start) = {
            let mut st = lock(&self.inner.state);
            st.settings = settings.clone();
            let same = st.listening.as_ref().map(|l| l.port) == Some(settings.port);
            if !settings.enabled {
                st.error = None;
                st.sessions.clear();
                st.stopped.clear();
                st.pairing = None;
                (st.listening.take(), false)
            } else if same {
                (None, false)
            } else {
                (st.listening.take(), true)
            }
        };
        if let Some(old) = old {
            let _ = old.shutdown.send(true);
            old.task.abort();
        }
        if start {
            let port = settings.port;
            match TcpListener::bind(("127.0.0.1", port)).await {
                Ok(listener) => {
                    let (tx, rx) = watch::channel(false);
                    let inner = self.inner.clone();
                    let task = tokio::spawn(accept_loop(inner, listener, port, rx));
                    let mut st = lock(&self.inner.state);
                    st.error = None;
                    st.listening = Some(Listening {
                        port,
                        shutdown: tx,
                        task,
                    });
                    tracing::info!("Chrome bridge listening on 127.0.0.1:{port}");
                }
                Err(e) => {
                    tracing::warn!("Chrome bridge couldn't listen on {port}: {e}");
                    let message = if e.kind() == std::io::ErrorKind::AddrInUse {
                        format!(
                            "Port {port} is in use by another program. Pick another port \
                             below (and the same one in the extension's popup)."
                        )
                    } else {
                        format!("Oxide couldn't listen on port {port}: {e}")
                    };
                    lock(&self.inner.state).error = Some(message);
                }
            }
        }
        self.inner.emit();
    }

    /// Stops listening and closes connections (on quit).
    pub async fn shutdown(&self) {
        let old = lock(&self.inner.state).listening.take();
        if let Some(old) = old {
            let _ = old.shutdown.send(true);
            old.task.abort();
        }
    }

    /// The port the bridge listens on now.
    pub fn port(&self) -> Option<u16> {
        lock(&self.inner.state).listening.as_ref().map(|l| l.port)
    }

    pub fn status(&self) -> ChromeStatus {
        self.inner.status()
    }

    /// Whether a paired Chrome is connected now.
    pub fn connected(&self) -> bool {
        !lock(&self.inner.state).conns.is_empty()
    }

    /// Shows a new six-word code (replacing any earlier one).
    pub fn start_pairing(&self) -> PairingCode {
        let code = words::new_code();
        let expires_at_ms = now_ms() + i64::try_from(PAIRING_TTL.as_millis()).unwrap_or(0);
        lock(&self.inner.state).pairing = Some(Pending {
            code: code.clone(),
            expires_at: Instant::now() + PAIRING_TTL,
            expires_at_ms,
            tries_left: PAIRING_TRIES,
        });
        self.inner.emit();
        PairingCode {
            code,
            expires_at: expires_at_ms,
        }
    }

    pub fn cancel_pairing(&self) {
        lock(&self.inner.state).pairing = None;
        self.inner.emit();
    }

    /// Removes a pairing: its token stops working and its connection closes.
    pub async fn forget(&self, browser_id: &str) {
        let (conns, list) = {
            let mut st = lock(&self.inner.state);
            st.paired.retain(|p| p.id != browser_id);
            let (gone, keep): (Vec<Conn>, Vec<Conn>) = std::mem::take(&mut st.conns)
                .into_iter()
                .partition(|c| c.browser_id == browser_id);
            st.conns = keep;
            (gone, st.paired.clone())
        };
        for c in conns {
            let _ =
                c.tx.send(Outgoing::Close(
                    close::FORGOTTEN,
                    "Unpaired in Oxide".into(),
                ))
                .await;
        }
        let secrets = self.inner.secrets.clone();
        let key = token_key(browser_id);
        let _ = tokio::task::spawn_blocking(move || secrets.delete(&key)).await;
        self.inner.save(list).await;
        self.inner.emit();
    }

    /// Calls one of the extension's tools for the chat `session`.
    pub async fn call(
        &self,
        session: &str,
        tool: &str,
        arguments: Value,
    ) -> Result<McpToolResult, BridgeError> {
        let tx = lock(&self.inner.state)
            .conns
            .last()
            .map(|c| c.tx.clone())
            .ok_or(BridgeError::NotConnected)?;
        let new_session = lock(&self.inner.state).sessions.insert(session.to_owned());
        if new_session {
            self.inner.emit();
        }
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let message = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": { "session": session, "name": tool, "arguments": arguments },
        })
        .to_string();
        let started = Instant::now();
        let (reply, answer) = oneshot::channel();
        tx.send(Outgoing::Call { id, message, reply })
            .await
            .map_err(|_| BridgeError::Disconnected)?;
        let value = match tokio::time::timeout(CALL_TIMEOUT, answer).await {
            Err(_) => return Err(BridgeError::Timeout),
            Ok(Err(_)) => return Err(BridgeError::Disconnected),
            Ok(Ok(Err(e))) => return Err(BridgeError::Remote(e)),
            Ok(Ok(Ok(v))) => v,
        };
        let mut result = parse_result(&value)?;
        result.duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        Ok(result)
    }

    /// The chat is done with Chrome for now: the extension takes its banner down. Calls
    /// after this start a new session.
    pub async fn release(&self, session: &str) {
        let (was, tx) = {
            let mut st = lock(&self.inner.state);
            let was = st.sessions.remove(session);
            let stopped = st.stopped.remove(session);
            (was || stopped, st.conns.last().map(|c| c.tx.clone()))
        };
        if !was {
            return;
        }
        if let Some(tx) = tx {
            let m = json!({
                "jsonrpc": "2.0",
                "method": "session/release",
                "params": { "session": session },
            })
            .to_string();
            let _ = tx.send(Outgoing::Notify(m)).await;
        }
        self.inner.emit();
    }

    /// Waits up to 5 seconds until no Chrome is connected (tests).
    pub async fn wait_disconnected(&self) -> bool {
        let until = Instant::now() + Duration::from_secs(5);
        while Instant::now() < until {
            if !self.connected() {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        !self.connected()
    }

    /// Waits until a paired Chrome is connected (tests).
    pub async fn wait_connected(&self, within: Duration) -> bool {
        let until = Instant::now() + within;
        while Instant::now() < until {
            if self.connected() {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        self.connected()
    }
}

/// A `tools/call` result (MCP-shaped: `content`, `isError`).
fn parse_result(v: &Value) -> Result<McpToolResult, BridgeError> {
    let content = v
        .get("content")
        .and_then(Value::as_array)
        .ok_or_else(|| BridgeError::BadAnswer("no content".into()))?;
    let content: Vec<McpContent> = content
        .iter()
        .filter_map(|c| serde_json::from_value(c.clone()).ok())
        .collect();
    Ok(McpToolResult {
        content,
        structured: None,
        is_error: v.get("isError").and_then(Value::as_bool).unwrap_or(false),
        duration_ms: 0,
    })
}

impl Inner {
    fn status(&self) -> ChromeStatus {
        let st = lock(&self.state);
        let active = st.conns.last();
        ChromeStatus {
            enabled: st.settings.enabled,
            listening: st.listening.is_some(),
            port: st.settings.port,
            error: st.error.clone(),
            connected: active.map(|c| c.name.clone()),
            extension_version: active.map(|c| c.version.clone()),
            paired: st.paired.clone(),
            pairing: st
                .pairing
                .as_ref()
                .filter(|p| p.expires_at > Instant::now())
                .map(|p| PairingCode {
                    code: p.code.clone(),
                    expires_at: p.expires_at_ms,
                }),
            sessions: st.sessions.iter().cloned().collect(),
            extension_dir: st.extension_dir.clone(),
        }
    }

    fn emit(&self) {
        self.sink.changed(&self.status());
    }

    async fn save(&self, paired: Vec<PairedBrowser>) {
        let body = match serde_json::to_vec_pretty(&PairingsFile { paired }) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!("couldn't encode the Chrome pairings: {e}");
                return;
            }
        };
        if let Some(dir) = self.file.parent() {
            let _ = tokio::fs::create_dir_all(dir).await;
        }
        if let Err(e) = tokio::fs::write(&self.file, body).await {
            tracing::warn!("couldn't save {}: {e}", self.file.display());
        }
    }

    /// Checks the code of a `pair` message (and uses it up when it is right).
    fn check_code(&self, given: Option<String>) -> Result<(), (i64, String)> {
        let mut st = lock(&self.state);
        let Some(p) = st
            .pairing
            .as_mut()
            .filter(|p| p.expires_at > Instant::now())
        else {
            st.pairing = None;
            return Err((
                codes::NO_PAIRING,
                "Oxide isn't waiting for a pairing. In Oxide, open Settings → Chrome and \
                 click Pair, then type the code shown there."
                    .into(),
            ));
        };
        if given.is_some_and(|g| same_secret(&g, &p.code)) {
            st.pairing = None;
            return Ok(());
        }
        p.tries_left = p.tries_left.saturating_sub(1);
        if p.tries_left == 0 {
            st.pairing = None;
            return Err((
                codes::WRONG_CODE,
                "Too many wrong codes: that code no longer works. Click Pair in Oxide for a \
                 new one."
                    .into(),
            ));
        }
        Err((
            codes::WRONG_CODE,
            "That isn't the code Oxide shows. Check the six words and try again.".into(),
        ))
    }

    /// Checks the first message. `Ok`: the browser, the extension's version and the result
    /// to answer with.
    async fn authenticate(
        &self,
        ext_id: &str,
        method: &str,
        params: &Value,
    ) -> Result<(PairedBrowser, String, Value), (i64, String)> {
        if method != "pair" && method != "hello" {
            return Err((
                codes::UNAUTHORIZED,
                "Pair first: the first message must be pair or hello.".into(),
            ));
        }
        let protocol = params.get("protocol").and_then(Value::as_u64).unwrap_or(0);
        if protocol != u64::from(PROTOCOL) {
            return Err((
                codes::PROTOCOL,
                "This extension doesn't match this version of Oxide. Load the extension from \
                 the folder Settings → Chrome shows, then pair again."
                    .into(),
            ));
        }
        let name: String = params
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("Chrome")
            .chars()
            .take(80)
            .collect();
        let version: String = params
            .get("version")
            .and_then(Value::as_str)
            .unwrap_or("")
            .chars()
            .take(40)
            .collect();
        if method == "pair" {
            let given = params
                .get("code")
                .and_then(Value::as_str)
                .and_then(oxide_core::chrome::clean_pairing_code);
            let checked = self.check_code(given);
            if checked.is_err() {
                self.emit();
            }
            checked?;
            let browser = PairedBrowser {
                id: uuid::Uuid::now_v7().to_string(),
                name,
                extension_id: ext_id.to_owned(),
                paired_at: now_ms(),
                last_seen: Some(now_ms()),
            };
            let token = new_token();
            let secrets = self.secrets.clone();
            let (key, value) = (token_key(&browser.id), Secret::new(token.clone()));
            let saved = tokio::task::spawn_blocking(move || secrets.set(&key, &value))
                .await
                .map_err(|e| e.to_string())
                .and_then(|r| r.map_err(|e| e.to_string()));
            if let Err(e) = saved {
                tracing::warn!("couldn't keep the Chrome token: {e}");
                return Err((
                    codes::UNAUTHORIZED,
                    format!("Oxide couldn't save the pairing in the keychain: {e}"),
                ));
            }
            let list = {
                let mut st = lock(&self.state);
                st.paired.push(browser.clone());
                st.paired.clone()
            };
            self.save(list).await;
            let result = json!({
                "browserId": browser.id,
                "token": token,
                "oxideVersion": oxide_core::version(),
            });
            return Ok((browser, version, result));
        }
        // hello
        let unauthorized = || {
            (
                codes::UNAUTHORIZED,
                "Oxide doesn't know this pairing (it may have been removed). Pair again: \
                 Settings → Chrome → Pair in Oxide."
                    .to_owned(),
            )
        };
        let id = params
            .get("browserId")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        let given = params.get("token").and_then(Value::as_str).unwrap_or("");
        let found = lock(&self.state)
            .paired
            .iter()
            .find(|p| p.id == id && p.extension_id == ext_id)
            .cloned();
        let Some(mut browser) = found else {
            return Err(unauthorized());
        };
        let secrets = self.secrets.clone();
        let key = token_key(&id);
        let stored = tokio::task::spawn_blocking(move || secrets.get(&key))
            .await
            .ok()
            .and_then(Result::ok)
            .flatten();
        if !stored.is_some_and(|t| same_secret(t.expose(), given)) {
            return Err(unauthorized());
        }
        browser.name = name;
        browser.last_seen = Some(now_ms());
        let list = {
            let mut st = lock(&self.state);
            if let Some(p) = st.paired.iter_mut().find(|p| p.id == browser.id) {
                *p = browser.clone();
            }
            st.paired.clone()
        };
        self.save(list).await;
        let result = json!({
            "browserId": browser.id,
            "oxideVersion": oxide_core::version(),
        });
        Ok((browser, version, result))
    }
}

async fn accept_loop(
    inner: Arc<Inner>,
    listener: TcpListener,
    port: u16,
    mut shutdown: watch::Receiver<bool>,
) {
    loop {
        tokio::select! {
            r = listener.accept() => match r {
                Ok((stream, peer)) => {
                    if !peer.ip().is_loopback() {
                        continue;
                    }
                    let inner = inner.clone();
                    let rx = shutdown.clone();
                    tokio::spawn(async move { serve(inner, stream, port, rx).await });
                }
                Err(e) => {
                    tracing::warn!("Chrome bridge accept failed: {e}");
                    tokio::time::sleep(Duration::from_millis(200)).await;
                }
            },
            _ = shutdown.changed() => break,
        }
    }
}

fn response(id: &Value, result: Value) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "result": result }).to_string()
}

fn error_response(id: &Value, code: i64, message: &str) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }).to_string()
}

type Writer = tokio::net::tcp::OwnedWriteHalf;

async fn send_text(w: &mut Writer, text: &str) -> Result<(), ws::WsError> {
    ws::write_frame(w, ws::OP_TEXT, text.as_bytes(), None).await
}

async fn send_close(w: &mut Writer, code: u16, reason: &str) {
    let _ = ws::write_frame(w, ws::OP_CLOSE, &ws::close_payload(code, reason), None).await;
    let _ = w.shutdown().await;
}

/// The first message of a connection, split into id, method and params.
fn split_request(text: &str) -> (Value, String, Value) {
    let parsed: Option<Value> = serde_json::from_str(text).ok();
    let field = |k: &str| parsed.as_ref().and_then(|v| v.get(k)).cloned();
    (
        field("id").unwrap_or(Value::Null),
        field("method")
            .and_then(|m| m.as_str().map(str::to_owned))
            .unwrap_or_default(),
        field("params").unwrap_or(Value::Null),
    )
}

/// One connection: handshake, authentication, then messages until either side goes.
async fn serve(
    inner: Arc<Inner>,
    stream: TcpStream,
    port: u16,
    mut shutdown: watch::Receiver<bool>,
) {
    let _ = stream.set_nodelay(true);
    let (rd, mut wr) = stream.into_split();
    let mut rd = BufReader::new(rd);
    let head = match tokio::time::timeout(Duration::from_secs(5), ws::read_head(&mut rd)).await {
        Ok(Ok(Some(h))) => h,
        Ok(Ok(None)) => {
            let _ = wr
                .write_all(ws::refusal(400, "Bad Request", "Bad request.").as_bytes())
                .await;
            return;
        }
        _ => return,
    };
    let (key, ext_id) = match check_upgrade(&head, port) {
        Ok(k) => k,
        Err(refusal) => {
            tracing::info!(
                "Chrome bridge refused a connection (origin {:?})",
                head.header("origin")
            );
            let _ = wr.write_all(refusal.as_bytes()).await;
            let _ = wr.shutdown().await;
            return;
        }
    };
    if wr.write_all(ws::switching(&key).as_bytes()).await.is_err() {
        return;
    }
    let (msg_tx, mut msg_rx) = mpsc::channel::<Result<Message, ws::WsError>>(32);
    let reader = tokio::spawn(async move {
        let mut r = ws::Reader::new(rd, true, MAX_MESSAGE);
        loop {
            let m = r.next().await;
            let end = m.is_err();
            if msg_tx.send(m).await.is_err() || end {
                break;
            }
        }
    });

    // Authentication: the first message, within the time limit.
    let first = tokio::time::timeout(AUTH_TIMEOUT, async {
        loop {
            match msg_rx.recv().await {
                Some(Ok(Message::Text(t))) => return Some(t),
                Some(Ok(Message::Ping(p))) => {
                    let _ = ws::write_frame(&mut wr, ws::OP_PONG, &p, None).await;
                }
                Some(Ok(Message::Pong(_) | Message::Binary(_))) => {}
                _ => return None,
            }
        }
    })
    .await;
    let first = match first {
        Ok(Some(t)) => t,
        Ok(None) => {
            reader.abort();
            return;
        }
        Err(_) => {
            send_close(&mut wr, close::POLICY, "Pair or say hello first").await;
            reader.abort();
            return;
        }
    };
    let (id, method, params) = split_request(&first);
    let (browser, version) = match inner.authenticate(&ext_id, &method, &params).await {
        Ok((browser, version, result)) => {
            if send_text(&mut wr, &response(&id, result)).await.is_err() {
                reader.abort();
                return;
            }
            (browser, version)
        }
        Err((code, message)) => {
            tracing::info!("Chrome bridge: {method} refused ({code})");
            let _ = send_text(&mut wr, &error_response(&id, code, &message)).await;
            send_close(&mut wr, close::UNAUTHORIZED, "Unauthorized").await;
            reader.abort();
            return;
        }
    };

    // Registered: tools may be called now.
    let conn_id = inner.next_id.fetch_add(1, Ordering::Relaxed);
    let (out_tx, mut out_rx) = mpsc::channel::<Outgoing>(32);
    let replaced: Vec<Conn> = {
        let mut st = lock(&inner.state);
        let (old, keep): (Vec<Conn>, Vec<Conn>) = std::mem::take(&mut st.conns)
            .into_iter()
            .partition(|c| c.browser_id == browser.id);
        st.conns = keep;
        st.conns.push(Conn {
            conn_id,
            browser_id: browser.id.clone(),
            name: browser.name.clone(),
            version,
            tx: out_tx,
        });
        old
    };
    for c in replaced {
        let _ =
            c.tx.send(Outgoing::Close(close::REPLACED, "Connected again".into()))
                .await;
    }
    tracing::info!("Chrome connected: {}", browser.name);
    inner.emit();

    let mut pending: HashMap<u64, oneshot::Sender<Result<Value, String>>> = HashMap::new();
    let mut beat = tokio::time::interval(HEARTBEAT);
    beat.tick().await;
    let mut heard = Instant::now();
    loop {
        tokio::select! {
            m = msg_rx.recv() => match m {
                Some(Ok(Message::Text(t))) => {
                    heard = Instant::now();
                    if let Some(answer) = handle_text(&inner, &t, &mut pending)
                        && send_text(&mut wr, &answer).await.is_err()
                    {
                        break;
                    }
                }
                Some(Ok(Message::Ping(p))) => {
                    heard = Instant::now();
                    if ws::write_frame(&mut wr, ws::OP_PONG, &p, None).await.is_err() {
                        break;
                    }
                }
                Some(Ok(Message::Pong(_) | Message::Binary(_))) => heard = Instant::now(),
                Some(Ok(Message::Close(_))) => {
                    send_close(&mut wr, close::NORMAL, "").await;
                    break;
                }
                Some(Err(ws::WsError::TooLarge)) => {
                    send_close(&mut wr, close::TOO_BIG, "Message too large").await;
                    break;
                }
                Some(Err(ws::WsError::Protocol(p))) => {
                    send_close(&mut wr, close::PROTOCOL_ERROR, p).await;
                    break;
                }
                Some(Err(_)) | None => break,
            },
            o = out_rx.recv() => match o {
                Some(Outgoing::Call { id, message, reply }) => {
                    pending.insert(id, reply);
                    if send_text(&mut wr, &message).await.is_err() {
                        break;
                    }
                }
                Some(Outgoing::Notify(m)) => {
                    if send_text(&mut wr, &m).await.is_err() {
                        break;
                    }
                }
                Some(Outgoing::Close(code, reason)) => {
                    send_close(&mut wr, code, &reason).await;
                    break;
                }
                None => break,
            },
            _ = beat.tick() => {
                if heard.elapsed() > IDLE_TIMEOUT {
                    send_close(&mut wr, close::GOING_AWAY, "No heartbeat").await;
                    break;
                }
                pending.retain(|_, r| !r.is_closed());
                if ws::write_frame(&mut wr, ws::OP_PING, b"oxide", None).await.is_err() {
                    break;
                }
            },
            _ = shutdown.changed() => {
                send_close(&mut wr, close::GOING_AWAY, "Oxide stopped listening").await;
                break;
            },
        }
    }
    reader.abort();
    // Dropping the waiting replies fails their calls with `Disconnected`.
    pending.clear();
    {
        let mut st = lock(&inner.state);
        st.conns.retain(|c| c.conn_id != conn_id);
        if st.conns.is_empty() {
            st.sessions.clear();
            st.stopped.clear();
        }
    }
    tracing::info!("Chrome disconnected: {}", browser.name);
    inner.emit();
}

/// A message from an authenticated extension. Returns what to answer, if anything.
fn handle_text(
    inner: &Inner,
    text: &str,
    pending: &mut HashMap<u64, oneshot::Sender<Result<Value, String>>>,
) -> Option<String> {
    let Ok(v) = serde_json::from_str::<Value>(text) else {
        return Some(error_response(&Value::Null, codes::PARSE, "Not JSON."));
    };
    let id = v.get("id").cloned();
    if let Some(method) = v.get("method").and_then(Value::as_str) {
        let params = v.get("params").cloned().unwrap_or(Value::Null);
        return match (method, id) {
            ("ping", Some(id)) => Some(response(&id, json!("pong"))),
            ("ping", None) => None,
            ("session/stopped", id) => {
                if let Some(s) = params.get("session").and_then(Value::as_str) {
                    let mut st = lock(&inner.state);
                    st.sessions.remove(s);
                    st.stopped.insert(s.to_owned());
                    drop(st);
                    inner.sink.stopped(s);
                    inner.emit();
                }
                id.map(|id| response(&id, Value::Null))
            }
            (_, Some(id)) => Some(error_response(&id, codes::NO_METHOD, "Unknown method.")),
            (_, None) => None,
        };
    }
    let Some(n) = id.as_ref().and_then(Value::as_u64) else {
        return Some(error_response(
            &Value::Null,
            codes::INVALID,
            "Not a JSON-RPC message.",
        ));
    };
    if let Some(reply) = pending.remove(&n) {
        let answer = match (v.get("result"), v.get("error")) {
            (Some(r), _) => Ok(r.clone()),
            (None, Some(e)) => Err(e
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("unknown error")
                .to_owned()),
            (None, None) => Err("an empty answer".to_owned()),
        };
        let _ = reply.send(answer);
    }
    None
}
