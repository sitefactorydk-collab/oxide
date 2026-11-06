//! Runs every server in `oxide_config.json`: one supervisor task per enabled server starts
//! it, restarts it with backoff when it dies, and stops it when it's disabled, removed or
//! Oxide quits. Keeps each server's state, lists and log, and forwards changes to a
//! [`ManagerSink`]. Server requests for the user (elicitation, sampling) become
//! [`McpInteraction`]s that the UI answers through [`McpManager::answer`].

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use futures_util::future::BoxFuture;
use oxide_core::{
    ImportPreview, McpAnswer, McpAsk, McpConfigFile, McpEra, McpEvent, McpInteraction, McpLogLine,
    McpPromptMessage, McpPromptResult, McpResourceRead, McpServerConfig, McpServerLists,
    McpServerState, McpStatus, McpToolResult, McpTransport,
};
use serde_json::{Map, Value, json};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinHandle;

use crate::McpError;
use crate::client::{ConnectOptions, McpClient};
use crate::config::{self, ConfigFile, OxideConfig};
use crate::handler::{ClientEvent, ClientHandler};
use crate::jsonrpc::RpcError;
use crate::log::{ServerLog, log_file_name, now_ms};
use crate::oauth::{self, OAuthSession, TokenStore};
use crate::process::{self, ProcessSpec};
use crate::protocol;
use crate::transport::{AuthProvider, HttpConfig, HttpTransport, StdioTransport, Transport};

/// Receives everything the UI shows.
pub trait ManagerSink: Send + Sync + 'static {
    fn event(&self, event: McpEvent);
}

/// What only the app can do for a server.
pub trait UserAgent: Send + Sync + 'static {
    /// Runs an approved `sampling/createMessage` with `model` (`None`: the default model) and
    /// returns its `CreateMessageResult`.
    fn sample(&self, params: Value, model: Option<String>) -> BoxFuture<'_, Result<Value, String>>;
    /// Opens a URL in the system browser.
    fn open_url(&self, url: &str) -> Result<(), String>;
    /// Folders the user shared with servers (`roots/list`).
    fn roots(&self) -> Vec<Value> {
        Vec::new()
    }
}

#[derive(Debug, Clone)]
pub struct ManagerOptions {
    pub connect: ConnectOptions,
    /// Failed starts in a row before Oxide stops retrying.
    pub max_attempts: u32,
    /// First retry delay; it doubles up to a minute.
    pub backoff: Duration,
    /// How long a question to the user stays open.
    pub ask_timeout: Duration,
    /// How often the config file is checked for edits.
    pub watch_every: Duration,
    /// Working folder of local servers (default: the home folder).
    pub cwd: Option<PathBuf>,
}

impl Default for ManagerOptions {
    fn default() -> Self {
        Self {
            connect: ConnectOptions::default(),
            max_attempts: 6,
            backoff: Duration::from_secs(1),
            ask_timeout: Duration::from_secs(600),
            watch_every: Duration::from_secs(1),
            cwd: None,
        }
    }
}

struct Slot {
    config: Option<McpServerConfig>,
    state: McpServerState,
    client: Option<Arc<McpClient>>,
    lists: McpServerLists,
    log: Arc<ServerLog>,
    stop: Option<watch::Sender<bool>>,
    task: Option<JoinHandle<()>>,
    /// Updates from supervisors of another generation are ignored.
    generation: u64,
    era_hint: Option<McpEra>,
    /// The last `WWW-Authenticate` challenge (for sign-in).
    challenge: Option<String>,
}

struct PendingAsk {
    interaction: McpInteraction,
    tx: oneshot::Sender<McpAnswer>,
}

struct Inner {
    file: Arc<ConfigFile>,
    logs_dir: Option<PathBuf>,
    sink: Arc<dyn ManagerSink>,
    agent: Arc<dyn UserAgent>,
    tokens: Arc<dyn TokenStore>,
    opts: ManagerOptions,
    config: Mutex<OxideConfig>,
    config_error: Mutex<Option<String>>,
    slots: Mutex<HashMap<String, Slot>>,
    order: Mutex<Vec<String>>,
    asks: Mutex<HashMap<String, PendingAsk>>,
    watcher: Mutex<Option<JoinHandle<()>>>,
    shutting_down: AtomicBool,
    next_gen: AtomicU64,
    /// The runtime servers run on, so they can be started from any thread (a synchronous
    /// Tauri command runs on the main thread, outside the runtime).
    runtime: Mutex<Option<tokio::runtime::Handle>>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// The MCP layer of the app. Cheap to clone.
#[derive(Clone)]
pub struct McpManager {
    inner: Arc<Inner>,
}

impl McpManager {
    pub fn new(
        config_path: PathBuf,
        logs_dir: Option<PathBuf>,
        sink: Arc<dyn ManagerSink>,
        agent: Arc<dyn UserAgent>,
        tokens: Arc<dyn TokenStore>,
        opts: ManagerOptions,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                file: ConfigFile::new(config_path),
                logs_dir,
                sink,
                agent,
                tokens,
                opts,
                config: Mutex::new(OxideConfig::default()),
                config_error: Mutex::new(None),
                slots: Mutex::new(HashMap::new()),
                order: Mutex::new(Vec::new()),
                asks: Mutex::new(HashMap::new()),
                watcher: Mutex::new(None),
                shutting_down: AtomicBool::new(false),
                next_gen: AtomicU64::new(1),
                runtime: Mutex::new(tokio::runtime::Handle::try_current().ok()),
            }),
        }
    }

    /// Loads the config, starts the enabled servers and watches the file for edits.
    pub async fn start(&self) -> Result<(), McpError> {
        *lock(&self.inner.runtime) = Some(tokio::runtime::Handle::current());
        let loaded = self.inner.file.load().await;
        let result = match loaded {
            Ok(cfg) => {
                self.inner.apply(cfg);
                Ok(())
            }
            Err(e) => {
                *lock(&self.inner.config_error) = Some(e.to_string());
                Err(e)
            }
        };
        let weak = Arc::downgrade(&self.inner);
        let task = self
            .inner
            .file
            .watch(self.inner.opts.watch_every, move |cfg| {
                let Some(inner) = weak.upgrade() else { return };
                match cfg {
                    Ok(cfg) => {
                        tracing::info!("MCP config changed; reloading");
                        *lock(&inner.config_error) = None;
                        inner.apply(cfg);
                    }
                    Err(e) => {
                        tracing::warn!("MCP config has an error: {e}");
                        *lock(&inner.config_error) = Some(e.to_string());
                    }
                }
            });
        if let Some(old) = lock(&self.inner.watcher).replace(task) {
            old.abort();
        }
        result
    }

    pub fn config_path(&self) -> PathBuf {
        self.inner.file.path().to_path_buf()
    }

    /// Why the config file couldn't be read, if it can't.
    pub fn config_error(&self) -> Option<String> {
        lock(&self.inner.config_error).clone()
    }

    pub fn states(&self) -> Vec<McpServerState> {
        let order = lock(&self.inner.order).clone();
        let slots = lock(&self.inner.slots);
        order
            .iter()
            .filter_map(|n| slots.get(n).map(|s| s.state.clone()))
            .collect()
    }

    pub fn state(&self, name: &str) -> Option<McpServerState> {
        lock(&self.inner.slots).get(name).map(|s| s.state.clone())
    }

    pub fn lists(&self, name: &str) -> Option<McpServerLists> {
        lock(&self.inner.slots).get(name).map(|s| s.lists.clone())
    }

    pub fn logs(&self, name: &str) -> Vec<McpLogLine> {
        lock(&self.inner.slots)
            .get(name)
            .map(|s| s.log.lines())
            .unwrap_or_default()
    }

    /// The configured entry for the edit form.
    pub fn server_config(&self, name: &str) -> Result<McpServerConfig, McpError> {
        match lock(&self.inner.config).server(name) {
            Some(Ok(c)) => Ok(c),
            Some(Err(why)) => Err(McpError::Config(why)),
            None => Err(McpError::Config(format!(
                "there's no server called \"{name}\""
            ))),
        }
    }

    /// Stops and starts a server again (also clears a "gave up" state).
    pub fn restart(&self, name: &str) -> Result<(), McpError> {
        let config = self.server_config(name)?;
        self.inner.stop_slot(name);
        self.inner.start_slot(name, config);
        Ok(())
    }

    fn client(&self, name: &str) -> Result<Arc<McpClient>, McpError> {
        lock(&self.inner.slots)
            .get(name)
            .and_then(|s| s.client.clone())
            .ok_or_else(|| McpError::Config(format!("\"{name}\" isn't running")))
    }

    pub async fn call_tool(
        &self,
        server: &str,
        tool: &str,
        arguments: Value,
        progress_token: Option<String>,
    ) -> Result<McpToolResult, McpError> {
        let client = self.client(server)?;
        let res = client
            .call_tool(tool, arguments, progress_token.as_deref())
            .await;
        if let Some(log) = lock(&self.inner.slots).get(server).map(|s| s.log.clone()) {
            match &res {
                Ok(r) => log.oxide(&format!(
                    "tools/call {tool}: {} in {} ms",
                    if r.is_error { "tool error" } else { "ok" },
                    r.duration_ms
                )),
                Err(e) => log.oxide(&format!("tools/call {tool} failed: {e}")),
            }
        }
        res
    }

    pub async fn read_resource(
        &self,
        server: &str,
        uri: &str,
    ) -> Result<McpResourceRead, McpError> {
        self.client(server)?.read_resource(uri).await
    }

    pub async fn get_prompt(
        &self,
        server: &str,
        prompt: &str,
        arguments: &Map<String, Value>,
    ) -> Result<McpPromptResult, McpError> {
        self.client(server)?.get_prompt(prompt, arguments).await
    }

    /// Questions from servers that are waiting for the user.
    pub fn pending_asks(&self) -> Vec<McpInteraction> {
        let mut v: Vec<McpInteraction> = lock(&self.inner.asks)
            .values()
            .map(|a| a.interaction.clone())
            .collect();
        v.sort_by_key(|i| i.created_at);
        v
    }

    /// Answers a question. `false` if it's no longer open.
    pub fn answer(&self, id: &str, answer: McpAnswer) -> bool {
        match lock(&self.inner.asks).remove(id) {
            Some(p) => p.tx.send(answer).is_ok(),
            None => false,
        }
    }

    // ----- Config editing -----

    pub async fn config_file(&self) -> Result<McpConfigFile, McpError> {
        self.inner.file.file().await
    }

    /// Saves the editor's text (must be valid) and applies it.
    pub async fn save_config_text(&self, text: &str) -> Result<(), McpError> {
        let cfg = self.inner.file.save_text(text).await?;
        *lock(&self.inner.config_error) = None;
        self.inner.apply(cfg);
        Ok(())
    }

    async fn edit(
        &self,
        f: impl FnOnce(&mut OxideConfig) -> Result<(), McpError>,
    ) -> Result<(), McpError> {
        let mut cfg = lock(&self.inner.config).clone();
        f(&mut cfg)?;
        self.inner.file.save(&cfg).await?;
        self.inner.apply(cfg);
        Ok(())
    }

    /// Adds a server, or replaces `old_name` (which may differ, to rename it).
    pub async fn save_server(
        &self,
        server: McpServerConfig,
        old_name: Option<String>,
    ) -> Result<(), McpError> {
        if old_name.is_none()
            && lock(&self.inner.config)
                .server(server.name.trim())
                .is_some()
        {
            return Err(McpError::Config(format!(
                "there's already a server called \"{}\"",
                server.name.trim()
            )));
        }
        self.edit(|c| c.upsert(&server, old_name.as_deref())).await
    }

    pub async fn remove_server(&self, name: &str) -> Result<(), McpError> {
        self.edit(|c| {
            c.remove(name);
            Ok(())
        })
        .await
    }

    pub async fn set_enabled(&self, name: &str, enabled: bool) -> Result<(), McpError> {
        self.edit(|c| {
            c.set_enabled(name, enabled);
            Ok(())
        })
        .await
    }

    /// What importing Claude desktop's servers would do.
    pub async fn import_preview(&self) -> ImportPreview {
        let ours = lock(&self.inner.config).clone();
        let paths = config::claude_desktop_paths();
        for p in &paths {
            if let Ok(text) = tokio::fs::read_to_string(p).await {
                return config::import_preview(&ours, p, Some(&text));
            }
        }
        let first = paths.into_iter().next().unwrap_or_default();
        config::import_preview(&ours, &first, None)
    }

    /// Copies the named servers from Claude desktop's config. Returns how many.
    pub async fn import(&self, names: Vec<String>) -> Result<usize, McpError> {
        let mut text = None;
        for p in config::claude_desktop_paths() {
            if let Ok(t) = tokio::fs::read_to_string(&p).await {
                text = Some(t);
                break;
            }
        }
        let text =
            text.ok_or_else(|| McpError::Config("Claude desktop's config wasn't found".into()))?;
        let mut count = 0;
        self.edit(|c| {
            count = config::import_into(c, &text, &names)?;
            Ok(())
        })
        .await?;
        Ok(count)
    }

    // ----- Sign-in -----

    /// Signs in to a remote server in the browser, then reconnects it.
    pub async fn sign_in(&self, name: &str) -> Result<(), McpError> {
        let config = self.server_config(name)?;
        if config.transport != McpTransport::Http {
            return Err(McpError::Config("only remote servers use sign-in".into()));
        }
        let challenge = lock(&self.inner.slots)
            .get(name)
            .and_then(|s| s.challenge.clone());
        let client = lock(&self.inner.config).oauth_client(name);
        let agent = self.inner.agent.clone();
        let open = move |u: &str| agent.open_url(u);
        if let Some(log) = lock(&self.inner.slots).get(name).map(|s| s.log.clone()) {
            log.oxide("signing in (browser opened)");
        }
        let result = oauth::sign_in(
            self.inner.tokens.as_ref(),
            oauth::SignIn {
                server: name,
                url: &config.url,
                challenge: challenge.as_deref(),
                client,
                timeout: Duration::from_secs(300),
            },
            &open,
        )
        .await;
        if let Some(log) = lock(&self.inner.slots).get(name).map(|s| s.log.clone()) {
            match &result {
                Ok(()) => log.oxide("signed in"),
                Err(e) => log.oxide(&format!("sign-in failed: {e}")),
            }
        }
        result?;
        self.restart(name)
    }

    pub async fn sign_out(&self, name: &str) -> Result<(), McpError> {
        self.inner
            .tokens
            .delete(&oauth::token_key(name))
            .await
            .map_err(McpError::Auth)?;
        self.restart(name)
    }

    /// Stops every server (closing stdin, then killing what doesn't exit) and the watcher.
    pub async fn shutdown(&self) {
        self.inner.shutting_down.store(true, Ordering::SeqCst);
        if let Some(w) = lock(&self.inner.watcher).take() {
            w.abort();
        }
        let tasks: Vec<JoinHandle<()>> = {
            let mut slots = lock(&self.inner.slots);
            slots
                .values_mut()
                .filter_map(|s| {
                    if let Some(stop) = s.stop.take() {
                        let _ = stop.send(true);
                    }
                    s.task.take()
                })
                .collect()
        };
        let all = futures_util::future::join_all(tasks);
        let _ = tokio::time::timeout(Duration::from_secs(8), all).await;
    }
}

impl Inner {
    fn next_generation(&self) -> u64 {
        self.next_gen.fetch_add(1, Ordering::Relaxed)
    }

    fn emit(&self, event: McpEvent) {
        self.sink.event(event);
    }

    /// Changes a slot if it still belongs to `generation`, and reports its new state.
    fn update(&self, name: &str, generation: u64, f: impl FnOnce(&mut Slot)) -> bool {
        let state = {
            let mut slots = lock(&self.slots);
            let Some(slot) = slots.get_mut(name) else {
                return false;
            };
            if slot.generation != generation {
                return false;
            }
            f(slot);
            slot.state.updated_at = now_ms();
            slot.state.clone()
        };
        self.emit(McpEvent::Server { state });
        true
    }

    fn new_log(self: &Arc<Self>, name: &str) -> Arc<ServerLog> {
        let sink = self.sink.clone();
        let server = name.to_owned();
        let file = self.logs_dir.as_ref().map(|d| d.join(log_file_name(name)));
        ServerLog::new(
            file,
            Some(Arc::new(move |line| {
                sink.event(McpEvent::Log {
                    server: server.clone(),
                    line,
                });
            })),
        )
    }

    fn blank_state(name: &str, config: Option<&McpServerConfig>) -> McpServerState {
        let (transport, summary) = match config {
            Some(c) if c.transport == McpTransport::Http => (McpTransport::Http, c.url.clone()),
            Some(c) => (
                McpTransport::Stdio,
                std::iter::once(c.command.as_str())
                    .chain(c.args.iter().map(String::as_str))
                    .collect::<Vec<_>>()
                    .join(" "),
            ),
            None => (McpTransport::Stdio, String::new()),
        };
        McpServerState {
            name: name.to_owned(),
            transport,
            summary,
            status: McpStatus::Disabled,
            error: None,
            era: None,
            protocol_version: None,
            server_name: None,
            server_version: None,
            instructions: None,
            capabilities: Default::default(),
            tools: 0,
            resources: 0,
            prompts: 0,
            restarts: 0,
            signed_in: false,
            updated_at: now_ms(),
        }
    }

    /// Brings the running servers in line with `cfg`.
    fn apply(self: &Arc<Self>, cfg: OxideConfig) {
        if self.shutting_down.load(Ordering::SeqCst) {
            return;
        }
        *lock(&self.config) = cfg.clone();
        let entries = cfg.servers();
        let names: Vec<String> = entries.iter().map(|(n, _)| n.clone()).collect();
        *lock(&self.order) = names.clone();

        // Removed servers.
        let gone: Vec<String> = lock(&self.slots)
            .keys()
            .filter(|k| !names.contains(k))
            .cloned()
            .collect();
        for name in gone {
            self.stop_slot(&name);
            lock(&self.slots).remove(&name);
            self.emit(McpEvent::Removed { name });
        }

        for (name, entry) in entries {
            match entry {
                Err(why) => {
                    self.stop_slot(&name);
                    let generation = self.ensure_slot(&name, None);
                    self.update(&name, generation, |s| {
                        s.config = None;
                        s.state = Self::blank_state(&name, None);
                        s.state.status = McpStatus::Error;
                        s.state.error = Some(format!("This entry can't be used: {why}"));
                    });
                }
                Ok(config) => {
                    let same = lock(&self.slots)
                        .get(&name)
                        .is_some_and(|s| s.config.as_ref() == Some(&config));
                    if same {
                        continue;
                    }
                    self.stop_slot(&name);
                    if config.enabled {
                        self.start_slot(&name, config);
                    } else {
                        let generation = self.ensure_slot(&name, Some(&config));
                        self.update(&name, generation, |s| {
                            s.config = Some(config.clone());
                            let restarts = s.state.restarts;
                            s.state = Self::blank_state(&name, Some(&config));
                            s.state.restarts = restarts;
                            s.lists = McpServerLists::default();
                        });
                    }
                }
            }
        }
    }

    /// Makes sure a slot exists and gives it a new generation, which it returns.
    fn ensure_slot(self: &Arc<Self>, name: &str, config: Option<&McpServerConfig>) -> u64 {
        let generation = self.next_generation();
        let mut slots = lock(&self.slots);
        match slots.get_mut(name) {
            Some(s) => s.generation = generation,
            None => {
                slots.insert(
                    name.to_owned(),
                    Slot {
                        config: config.cloned(),
                        state: Self::blank_state(name, config),
                        client: None,
                        lists: McpServerLists::default(),
                        log: self.new_log(name),
                        stop: None,
                        task: None,
                        generation,
                        era_hint: None,
                        challenge: None,
                    },
                );
            }
        }
        generation
    }

    /// Tells a slot's supervisor to stop; its later updates are ignored.
    fn stop_slot(&self, name: &str) {
        let generation = self.next_generation();
        let mut slots = lock(&self.slots);
        if let Some(s) = slots.get_mut(name) {
            s.generation = generation;
            if let Some(stop) = s.stop.take() {
                let _ = stop.send(true);
            }
            s.task = None;
            s.client = None;
        }
    }

    fn start_slot(self: &Arc<Self>, name: &str, config: McpServerConfig) {
        let generation = self.ensure_slot(name, Some(&config));
        let (stop_tx, stop_rx) = watch::channel(false);
        let inner = self.clone();
        let task_name = name.to_owned();
        let task_config = config.clone();
        let runtime = tokio::runtime::Handle::try_current()
            .ok()
            .or_else(|| lock(&self.runtime).clone());
        let Some(runtime) = runtime else {
            tracing::error!("MCP server {name} can't start: no async runtime");
            return;
        };
        let task = runtime.spawn(async move {
            supervise(inner, task_name, generation, task_config, stop_rx).await;
        });
        self.update(name, generation, move |s| {
            let restarts = s.state.restarts;
            s.state = Self::blank_state(name, Some(&config));
            s.state.status = McpStatus::Starting;
            s.state.restarts = restarts;
            s.config = Some(config);
            s.stop = Some(stop_tx);
            s.task = Some(task);
            s.lists = McpServerLists::default();
        });
    }

    /// Asks the user something on behalf of a server and waits for the answer.
    async fn ask(&self, server: &str, ask: McpAsk) -> Option<McpAnswer> {
        let id = format!("ask-{}", self.next_generation());
        let interaction = McpInteraction {
            id: id.clone(),
            server: server.to_owned(),
            ask,
            created_at: now_ms(),
        };
        let (tx, rx) = oneshot::channel();
        lock(&self.asks).insert(
            id.clone(),
            PendingAsk {
                interaction: interaction.clone(),
                tx,
            },
        );
        self.emit(McpEvent::Ask { interaction });
        let answer = tokio::time::timeout(self.opts.ask_timeout, rx)
            .await
            .ok()
            .and_then(Result::ok);
        lock(&self.asks).remove(&id);
        self.emit(McpEvent::AskClosed { id });
        answer
    }
}

async fn supervise(
    inner: Arc<Inner>,
    name: String,
    generation: u64,
    config: McpServerConfig,
    mut stop: watch::Receiver<bool>,
) {
    let Some(log) = lock(&inner.slots).get(&name).map(|s| s.log.clone()) else {
        return;
    };
    let mut attempt: u32 = 0;
    loop {
        if *stop.borrow() {
            break;
        }
        let prefer = lock(&inner.slots).get(&name).and_then(|s| s.era_hint);
        inner.update(&name, generation, |s| {
            s.state.status = McpStatus::Starting;
        });
        let connecting = connect_server(&inner, &name, &config, prefer, log.clone(), generation);
        let res = tokio::select! {
            r = connecting => r,
            _ = stop.changed() => break,
        };
        match res {
            Ok(client) => {
                let started = Instant::now();
                let info = client.info().clone();
                inner.update(&name, generation, |s| {
                    s.client = Some(client.clone());
                    s.era_hint = Some(info.era);
                    s.challenge = None;
                    s.state.status = McpStatus::Running;
                    s.state.error = None;
                    s.state.era = Some(info.era);
                    s.state.protocol_version = Some(info.version.clone());
                    s.state.server_name = info.name.clone();
                    s.state.server_version = info.server_version.clone();
                    s.state.instructions = info.instructions.clone();
                    s.state.capabilities = protocol::capabilities_summary(&info.capabilities);
                });
                log.oxide(&format!(
                    "connected: {} {} (protocol {}, {})",
                    info.name.as_deref().unwrap_or("server"),
                    info.server_version.as_deref().unwrap_or(""),
                    info.version,
                    match info.era {
                        McpEra::Modern => "stateless",
                        McpEra::Legacy => "legacy handshake",
                    }
                ));
                refresh_lists(&inner, &name, generation).await;
                let reason = tokio::select! {
                    r = wait_closed(client.closed()) => r,
                    _ = stop.changed() => {
                        client.close().await;
                        break;
                    }
                };
                client.close().await;
                log.oxide(&format!("connection lost{reason}"));
                if started.elapsed() > Duration::from_secs(60) {
                    attempt = 0;
                }
                inner.update(&name, generation, |s| {
                    s.client = None;
                    s.state.restarts += 1;
                    s.state.status = McpStatus::Error;
                    s.state.error = Some(format!("The server stopped{reason}. Restarting…"));
                });
            }
            Err(McpError::Unauthorized { challenge }) => {
                log.oxide("the server needs a sign-in");
                inner.update(&name, generation, |s| {
                    s.challenge = challenge;
                    s.state.status = McpStatus::NeedsAuth;
                    s.state.error = Some("Sign in to connect this server.".into());
                    s.state.signed_in = false;
                });
                let _ = stop.changed().await;
                break;
            }
            Err(e) => {
                log.oxide(&format!("couldn't connect: {e}"));
                let fatal = matches!(e, McpError::Config(_) | McpError::Unsupported(_));
                if matches!(e, McpError::Closed(_)) && prefer.is_none() {
                    // It died on the modern probe: an older server that exits on unknown
                    // methods. Skip the probe next time.
                    inner.update(&name, generation, |s| s.era_hint = Some(McpEra::Legacy));
                }
                inner.update(&name, generation, |s| {
                    s.state.status = McpStatus::Error;
                    s.state.error = Some(capitalize(&e.to_string()));
                });
                if fatal {
                    let _ = stop.changed().await;
                    break;
                }
            }
        }
        attempt += 1;
        if attempt >= inner.opts.max_attempts {
            log.oxide(&format!("gave up after {attempt} attempts"));
            inner.update(&name, generation, |s| {
                let why = s.state.error.clone().unwrap_or_default();
                s.state.error = Some(format!(
                    "{} Oxide stopped retrying; use Restart to try again.",
                    why.trim_end_matches(" Restarting…")
                ));
            });
            let _ = stop.changed().await;
            break;
        }
        let factor = 2u32.saturating_pow(attempt.saturating_sub(1).min(6));
        let delay = (inner.opts.backoff * factor).min(Duration::from_secs(60));
        tokio::select! {
            _ = tokio::time::sleep(delay) => {}
            _ = stop.changed() => break,
        }
    }
    if inner.shutting_down.load(Ordering::SeqCst) {
        inner.update(&name, generation, |s| {
            s.client = None;
            s.state.status = McpStatus::Stopped;
        });
    }
}

/// Waits until a connection is gone and returns why.
async fn wait_closed(mut rx: watch::Receiver<Option<String>>) -> String {
    let _ = rx.wait_for(Option::is_some).await;
    let reason = rx.borrow().clone();
    reason.unwrap_or_default()
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

async fn connect_server(
    inner: &Arc<Inner>,
    name: &str,
    config: &McpServerConfig,
    prefer: Option<McpEra>,
    log: Arc<ServerLog>,
    generation: u64,
) -> Result<Arc<McpClient>, McpError> {
    let (tx, rx) = mpsc::unbounded_channel();
    let transport = match config.transport {
        McpTransport::Stdio => Transport::Stdio(StdioTransport::spawn(
            &ProcessSpec {
                command: config.command.clone(),
                args: config.args.clone(),
                env: config.env.clone(),
                cwd: inner.opts.cwd.clone().or_else(process::home_dir),
            },
            tx,
            log,
        )?),
        McpTransport::Http => {
            let session = OAuthSession::load(inner.tokens.clone(), name, &config.url).await;
            let signed_in = session.is_some();
            inner.update(name, generation, |s| s.state.signed_in = signed_in);
            let auth = session.map(|s| s as Arc<dyn AuthProvider>);
            log.oxide(&format!("connecting to {}", config.url));
            Transport::Http(HttpTransport::new(
                &HttpConfig {
                    url: config.url.clone(),
                    headers: config
                        .headers
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect(),
                },
                auth,
                tx,
                log,
            )?)
        }
    };
    let handler = Arc::new(Bridge {
        inner: Arc::downgrade(inner),
        generation,
    });
    let opts = ConnectOptions {
        prefer,
        ..inner.opts.connect.clone()
    };
    McpClient::connect(name, transport, rx, handler, opts).await
}

async fn refresh_lists(inner: &Arc<Inner>, name: &str, generation: u64) {
    let Some((client, log)) = lock(&inner.slots)
        .get(name)
        .filter(|s| s.generation == generation)
        .and_then(|s| s.client.clone().map(|c| (c, s.log.clone())))
    else {
        return;
    };
    let note = |what: &str, e: McpError| {
        log.oxide(&format!("couldn't list {what}: {e}"));
    };
    let tools = client.list_tools().await.unwrap_or_else(|e| {
        note("tools", e);
        Vec::new()
    });
    let resources = client.list_resources().await.unwrap_or_else(|e| {
        note("resources", e);
        Vec::new()
    });
    let templates = client.list_resource_templates().await.unwrap_or_else(|e| {
        note("resource templates", e);
        Vec::new()
    });
    let prompts = client.list_prompts().await.unwrap_or_else(|e| {
        note("prompts", e);
        Vec::new()
    });
    let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    let updated = inner.update(name, generation, |s| {
        s.state.tools = count(tools.len());
        s.state.resources = count(resources.len());
        s.state.prompts = count(prompts.len());
        s.lists = McpServerLists {
            tools,
            resources,
            templates,
            prompts,
        };
    });
    if updated {
        inner.emit(McpEvent::Lists {
            server: name.to_owned(),
        });
    }
}

/// The [`ClientHandler`] each client gets: forwards to the manager without keeping it alive.
struct Bridge {
    inner: Weak<Inner>,
    generation: u64,
}

fn sampling_messages(params: &Value) -> Vec<McpPromptMessage> {
    let mut out = Vec::new();
    for m in params
        .get("messages")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let role = m
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("user")
            .to_owned();
        match m.get("content") {
            Some(Value::Array(parts)) => {
                for p in parts {
                    out.push(McpPromptMessage {
                        role: role.clone(),
                        content: protocol::parse_content(p),
                    });
                }
            }
            Some(c) => out.push(McpPromptMessage {
                role,
                content: protocol::parse_content(c),
            }),
            None => {}
        }
    }
    out
}

impl ClientHandler for Bridge {
    fn sample<'a>(
        &'a self,
        server: &'a str,
        params: Value,
    ) -> BoxFuture<'a, Result<Value, RpcError>> {
        Box::pin(async move {
            let Some(inner) = self.inner.upgrade() else {
                return Err(RpcError::new(-1, "Oxide is shutting down"));
            };
            let ask = McpAsk::Sampling {
                system_prompt: params
                    .get("systemPrompt")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                messages: sampling_messages(&params),
                max_tokens: params
                    .get("maxTokens")
                    .and_then(Value::as_u64)
                    .and_then(|n| u32::try_from(n).ok())
                    .unwrap_or(1024),
                model_hints: params
                    .pointer("/modelPreferences/hints")
                    .and_then(Value::as_array)
                    .map(|h| {
                        h.iter()
                            .filter_map(|x| x.get("name").and_then(Value::as_str))
                            .map(str::to_owned)
                            .collect()
                    })
                    .unwrap_or_default(),
            };
            match inner.ask(server, ask).await {
                Some(McpAnswer::Sampling {
                    approved: true,
                    model,
                }) => inner
                    .agent
                    .sample(params, model)
                    .await
                    .map_err(|e| RpcError::new(-1, e)),
                _ => Err(RpcError::new(-1, "The user declined the sampling request")),
            }
        })
    }

    fn elicit<'a>(
        &'a self,
        server: &'a str,
        params: Value,
    ) -> BoxFuture<'a, Result<Value, RpcError>> {
        Box::pin(async move {
            let Some(inner) = self.inner.upgrade() else {
                return Ok(json!({"action": "cancel"}));
            };
            let url = params.get("url").and_then(Value::as_str).map(str::to_owned);
            let ask = McpAsk::Elicitation {
                message: params
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("The server needs more information.")
                    .to_owned(),
                schema: params.get("requestedSchema").map(Value::to_string),
                url: url.clone(),
            };
            match inner.ask(server, ask).await {
                Some(McpAnswer::Elicitation { action, content }) => {
                    let action = match action.as_str() {
                        "accept" | "decline" | "cancel" => action,
                        _ => "cancel".to_owned(),
                    };
                    if action == "accept" {
                        if let Some(u) = &url {
                            if let Err(e) = inner.agent.open_url(u) {
                                tracing::warn!("couldn't open an elicitation URL: {e}");
                            }
                            return Ok(json!({"action": "accept"}));
                        }
                        let content = content
                            .and_then(|c| serde_json::from_str::<Value>(&c).ok())
                            .unwrap_or_else(|| json!({}));
                        return Ok(json!({"action": "accept", "content": content}));
                    }
                    Ok(json!({"action": action}))
                }
                _ => Ok(json!({"action": "cancel"})),
            }
        })
    }

    fn roots<'a>(&'a self, _server: &'a str) -> BoxFuture<'a, Vec<Value>> {
        Box::pin(async move {
            self.inner
                .upgrade()
                .map(|i| i.agent.roots())
                .unwrap_or_default()
        })
    }

    fn event(&self, server: &str, event: ClientEvent) {
        let Some(inner) = self.inner.upgrade() else {
            return;
        };
        match event {
            ClientEvent::Log { level, text } => {
                if let Some(log) = lock(&inner.slots).get(server).map(|s| s.log.clone()) {
                    log.push(oxide_core::McpLogSource::Server, Some(&level), &text);
                }
            }
            ClientEvent::Progress {
                token,
                progress,
                total,
                message,
            } => inner.emit(McpEvent::Progress {
                token,
                progress,
                total,
                message,
            }),
            ClientEvent::ListChanged => {
                let name = server.to_owned();
                let generation = self.generation;
                tokio::spawn(async move { refresh_lists(&inner, &name, generation).await });
            }
            ClientEvent::ResourceUpdated { .. } => inner.emit(McpEvent::Lists {
                server: server.to_owned(),
            }),
        }
    }
}
