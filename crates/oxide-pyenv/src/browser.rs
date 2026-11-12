//! The browser agent's processes (Phase 13).
//!
//! Each chat that uses the browser gets its own server: `python -m oxide_browser_agent` in
//! the managed environment, spoken to with Oxide's own MCP client (`oxide-mcp`). The server
//! starts a browser of its own on first use and closes it when the server stops: when the
//! chat is deleted, when the user closes it, after `idle_minutes` without use, when the
//! settings or the model change, and when Oxide quits.
//!
//! Setting up means installing the `browser` dependency group into the managed
//! environment and, when no Chrome or Edge is installed (or Settings asks for it),
//! Playwright's Chromium into `<python root>/browsers`.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures_util::future::BoxFuture;
use oxide_core::browser::{
    BROWSER_MODULES, BROWSER_PACKAGE_GROUP, BROWSER_SERVER, BrowserFrame, BrowserSettings,
    BrowserSource, BrowserStatus, BrowserStep, InstalledBrowser,
};
use oxide_core::{McpToolResult, ProviderKind};
use oxide_mcp::handler::{ClientEvent, ClientHandler};
use oxide_mcp::jsonrpc::RpcError;
use oxide_mcp::log::ServerLog;
use oxide_mcp::process::ProcessSpec;
use oxide_mcp::transport::{StdioTransport, Transport};
use oxide_mcp::{ConnectOptions, McpClient, McpError};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::io::AsyncReadExt;

use crate::env::{PythonEnv, remove_dir};
use crate::{PythonError, process, project};

/// How long a server may take to start and answer.
const CONNECT: Duration = Duration::from_secs(90);
/// Longest the Chromium download may take.
const DOWNLOAD_LIMIT: Duration = Duration::from_secs(30 * 60);
/// Lines of setup output kept for the UI.
const LOG_LINES: usize = 40;
/// Least time between two status events while downloading.
const EMIT_EVERY: Duration = Duration::from_millis(250);
/// The module/distribution pairs that must import for the agent to run.
const CHECK: [(&str, &str); 3] = [
    (BROWSER_MODULES[0], "mcp"),
    (BROWSER_MODULES[1], "playwright"),
    (BROWSER_MODULES[2], "browser-use"),
];

/// Something went wrong; the text is written for the model and the user.
#[derive(Debug, thiserror::Error)]
pub enum BrowserError {
    #[error("{0}")]
    Python(#[from] PythonError),
    #[error("Setting up the browser failed: {0}")]
    Setup(String),
    #[error("The browser agent didn't start: {0}")]
    Start(String),
    #[error("The browser agent failed: {0}")]
    Call(String),
}

/// Told about setup progress and live screenshots.
pub trait BrowserSink: Send + Sync + 'static {
    fn status(&self, status: &BrowserStatus);
    fn frame(&self, frame: BrowserFrame);
}

/// Ignores everything.
pub struct NoBrowserSink;

impl BrowserSink for NoBrowserSink {
    fn status(&self, _: &BrowserStatus) {}
    fn frame(&self, _: BrowserFrame) {}
}

/// The model `browser_task` runs with: the chat's (or Settings') endpoint.
#[derive(Clone, PartialEq, Eq)]
pub struct BrowserLlm {
    pub kind: ProviderKind,
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
    /// The model takes images (Browser-Use then sends it screenshots).
    pub vision: bool,
}

impl std::fmt::Debug for BrowserLlm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BrowserLlm")
            .field("kind", &self.kind)
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .field("api_key", &self.api_key.as_ref().map(|_| "…"))
            .field("vision", &self.vision)
            .finish()
    }
}

impl BrowserLlm {
    /// Identifies the model without containing the key (a hash of it).
    fn fingerprint(&self) -> String {
        format!(
            "{}|{}|{}|{}|{}",
            self.kind.as_str(),
            self.base_url,
            self.model,
            self.vision,
            fnv(self.api_key.as_deref().unwrap_or(""))
        )
    }
}

fn fnv(s: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// Reads a progress message of the browser agent: JSON with `text`, `url` and `frame`
/// (base64 JPEG), or plain text from anything else.
pub fn parse_frame(conversation_id: &str, tool_use_id: &str, message: &str) -> BrowserFrame {
    #[derive(Deserialize)]
    struct Msg {
        text: Option<String>,
        url: Option<String>,
        frame: Option<String>,
    }
    let parsed = if message.trim_start().starts_with('{') {
        serde_json::from_str::<Msg>(message).ok()
    } else {
        None
    };
    let (text, url, image) = match parsed {
        Some(m) => (m.text.unwrap_or_default(), m.url, m.frame),
        None => (message.to_owned(), None, None),
    };
    BrowserFrame {
        conversation_id: conversation_id.to_owned(),
        tool_use_id: tool_use_id.to_owned(),
        text,
        url: url.filter(|u| !u.is_empty()),
        image: image.filter(|f| !f.is_empty()),
    }
}

/// Chrome, Edge and Chromium installed in their usual places (Chrome first).
pub fn find_installed_browsers() -> Vec<InstalledBrowser> {
    let mut found: Vec<InstalledBrowser> = Vec::new();
    let mut add = |name: &str, path: PathBuf| {
        if path.is_file() && !found.iter().any(|b| Path::new(&b.path) == path) {
            found.push(InstalledBrowser {
                name: name.to_owned(),
                path: path.to_string_lossy().into_owned(),
            });
        }
    };
    if cfg!(windows) {
        let dirs: Vec<PathBuf> = ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"]
            .iter()
            .filter_map(|v| std::env::var_os(v).map(PathBuf::from))
            .collect();
        for d in &dirs {
            add(
                "Google Chrome",
                d.join("Google")
                    .join("Chrome")
                    .join("Application")
                    .join("chrome.exe"),
            );
        }
        for d in &dirs {
            add(
                "Microsoft Edge",
                d.join("Microsoft")
                    .join("Edge")
                    .join("Application")
                    .join("msedge.exe"),
            );
        }
    } else if cfg!(target_os = "macos") {
        for (name, app) in [
            ("Google Chrome", "Google Chrome"),
            ("Microsoft Edge", "Microsoft Edge"),
            ("Chromium", "Chromium"),
        ] {
            add(
                name,
                PathBuf::from(format!("/Applications/{app}.app/Contents/MacOS/{app}")),
            );
        }
    } else {
        let path = std::env::var_os("PATH").unwrap_or_default();
        for (name, exe) in [
            ("Google Chrome", "google-chrome"),
            ("Google Chrome", "google-chrome-stable"),
            ("Microsoft Edge", "microsoft-edge"),
            ("Chromium", "chromium"),
            ("Chromium", "chromium-browser"),
        ] {
            if let Some(p) = std::env::split_paths(&path)
                .map(|d| d.join(exe))
                .find(|p| p.is_file())
            {
                add(name, p);
            }
        }
    }
    found
}

/// Reads a download percentage from Playwright's progress output (`|■■■   |  30% of 150 MiB`).
fn percent_of(line: &str) -> Option<u32> {
    let at = line.find('%')?;
    let digits: String = line[..at]
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    digits.parse::<u32>().ok().filter(|p| *p <= 100)
}

#[derive(Debug, Serialize, Deserialize)]
struct ChromiumMarker {
    path: String,
}

#[derive(Default)]
struct Live {
    installing: bool,
    step: Option<BrowserStep>,
    message: Option<String>,
    percent: Option<u32>,
    error: Option<String>,
    log: VecDeque<String>,
    last_emit: Option<Instant>,
}

struct Server {
    client: Arc<McpClient>,
    key: String,
    last_used: Instant,
}

struct Inner {
    env: PythonEnv,
    sink: Arc<dyn BrowserSink>,
    settings: Mutex<BrowserSettings>,
    servers: Mutex<HashMap<String, Server>>,
    /// One server start per chat at a time.
    starting: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    /// One setup at a time.
    setup: tokio::sync::Mutex<()>,
    live: Mutex<Live>,
    logs: Option<PathBuf>,
    /// Browsers to use instead of looking (tests).
    installed: Option<Vec<InstalledBrowser>>,
    reaper: AtomicBool,
}

/// The browser agents of all chats. Cheap to clone.
#[derive(Clone)]
pub struct BrowserAgents(Arc<Inner>);

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// What a chat's server is started with.
struct Launch {
    executable: Option<String>,
    using: String,
}

/// Answers the agent's requests to Oxide (it makes none) and turns its progress notices
/// into live frames.
struct FrameHandler {
    chat: String,
    sink: Arc<dyn BrowserSink>,
}

impl ClientHandler for FrameHandler {
    fn sample<'a>(
        &'a self,
        _server: &'a str,
        _params: Value,
    ) -> BoxFuture<'a, Result<Value, RpcError>> {
        Box::pin(async { Err(RpcError::new(-1, "sampling isn't available")) })
    }

    fn elicit<'a>(
        &'a self,
        _server: &'a str,
        _params: Value,
    ) -> BoxFuture<'a, Result<Value, RpcError>> {
        Box::pin(async { Ok(json!({"action": "decline"})) })
    }

    fn roots<'a>(&'a self, _server: &'a str) -> BoxFuture<'a, Vec<Value>> {
        Box::pin(async { Vec::new() })
    }

    fn event(&self, _server: &str, event: ClientEvent) {
        if let ClientEvent::Progress {
            token,
            message: Some(message),
            ..
        } = event
        {
            self.sink.frame(parse_frame(&self.chat, &token, &message));
        }
    }
}

impl BrowserAgents {
    /// `logs`: the folder for the servers' stderr log (`browser-agent.log`).
    pub fn new(env: PythonEnv, sink: Arc<dyn BrowserSink>, logs: Option<PathBuf>) -> Self {
        Self(Arc::new(Inner {
            env,
            sink,
            settings: Mutex::new(BrowserSettings::default()),
            servers: Mutex::new(HashMap::new()),
            starting: Mutex::new(HashMap::new()),
            setup: tokio::sync::Mutex::new(()),
            live: Mutex::new(Live::default()),
            logs,
            installed: None,
            reaper: AtomicBool::new(false),
        }))
    }

    /// Uses these browsers instead of looking for installed ones (tests).
    #[doc(hidden)]
    pub fn with_installed(
        env: PythonEnv,
        installed: Vec<InstalledBrowser>,
        sink: Arc<dyn BrowserSink>,
    ) -> Self {
        Self(Arc::new(Inner {
            env,
            sink,
            settings: Mutex::new(BrowserSettings::default()),
            servers: Mutex::new(HashMap::new()),
            starting: Mutex::new(HashMap::new()),
            setup: tokio::sync::Mutex::new(()),
            live: Mutex::new(Live::default()),
            logs: None,
            installed: Some(installed),
            reaper: AtomicBool::new(false),
        }))
    }

    pub fn configure(&self, settings: &BrowserSettings) {
        *lock(&self.0.settings) = settings.clone().normalized();
    }

    pub fn settings(&self) -> BrowserSettings {
        lock(&self.0.settings).clone()
    }

    pub fn python(&self) -> &PythonEnv {
        &self.0.env
    }

    fn browsers_dir(&self) -> PathBuf {
        self.0.env.root().join("browsers")
    }

    fn agents_dir(&self) -> PathBuf {
        self.0.env.root().join("agents")
    }

    fn chromium_marker(&self) -> PathBuf {
        self.browsers_dir().join("oxide-chromium.json")
    }

    fn installed(&self) -> Vec<InstalledBrowser> {
        match &self.0.installed {
            Some(list) => list.clone(),
            None => find_installed_browsers(),
        }
    }

    /// The Chromium Oxide downloaded, if it is still there.
    async fn chromium(&self) -> Option<String> {
        let text = tokio::fs::read_to_string(self.chromium_marker())
            .await
            .ok()?;
        let m: ChromiumMarker = serde_json::from_str(&text).ok()?;
        tokio::fs::metadata(&m.path)
            .await
            .ok()
            .filter(|md| md.is_file())
            .map(|_| m.path)
    }

    /// Chats whose browser agent runs now.
    pub fn open_chats(&self) -> Vec<String> {
        let mut v: Vec<String> = lock(&self.0.servers).keys().cloned().collect();
        v.sort();
        v
    }

    pub fn is_open(&self, chat: &str) -> bool {
        lock(&self.0.servers).contains_key(chat)
    }

    // ---- Status -----------------------------------------------------------------------------

    fn live_status(&self) -> BrowserStatus {
        let live = lock(&self.0.live);
        BrowserStatus {
            installing: live.installing,
            step: live.step,
            message: live.message.clone(),
            percent: live.percent,
            error: live.error.clone(),
            log: live.log.iter().cloned().collect(),
            open_chats: self.open_chats(),
            browsers_dir: self.browsers_dir().to_string_lossy().into_owned(),
            ..BrowserStatus::default()
        }
    }

    /// What Settings → Browser shows.
    pub async fn status(&self) -> BrowserStatus {
        let mut s = self.live_status();
        let packages = self.0.env.group_ready(BROWSER_PACKAGE_GROUP).await;
        s.packages = packages.is_some();
        s.versions = packages.unwrap_or_default();
        s.chromium = self.chromium().await;
        let installed = self.installed();
        let settings = self.settings();
        s.using = match settings.source {
            BrowserSource::Cdp => Some(format!("Your Chrome at {}", settings.cdp_url)),
            BrowserSource::Auto if !installed.is_empty() => {
                installed.first().map(|b| b.name.clone())
            }
            _ => s
                .chromium
                .as_ref()
                .map(|_| "Chromium (downloaded by Oxide)".to_owned()),
        };
        s.installed = installed;
        s
    }

    fn emit(&self, force: bool) {
        {
            let mut live = lock(&self.0.live);
            let now = Instant::now();
            if !force
                && live
                    .last_emit
                    .is_some_and(|t| now.duration_since(t) < EMIT_EVERY)
            {
                return;
            }
            live.last_emit = Some(now);
        }
        self.0.sink.status(&self.live_status());
    }

    async fn emit_settled(&self) {
        let s = self.status().await;
        lock(&self.0.live).last_emit = Some(Instant::now());
        self.0.sink.status(&s);
    }

    fn step(&self, step: BrowserStep, message: &str) {
        {
            let mut live = lock(&self.0.live);
            live.step = Some(step);
            live.message = Some(message.to_owned());
            live.percent = None;
            push(&mut live.log, format!("▸ {message}"));
        }
        self.emit(true);
    }

    // ---- Setup ------------------------------------------------------------------------------

    /// Installs what the browser needs: Python and its packages, and Chromium when the
    /// settings need it (or `chromium` asks for it). Concurrent calls share one setup.
    pub async fn setup(&self, chromium: bool) -> Result<(), BrowserError> {
        self.run_setup(chromium).await.map(|_| ())
    }

    async fn run_setup(&self, force_chromium: bool) -> Result<Launch, BrowserError> {
        let _one = self.0.setup.lock().await;
        {
            let mut live = lock(&self.0.live);
            live.installing = true;
            live.error = None;
            live.log.clear();
        }
        self.emit(true);
        let result = self.install(force_chromium).await;
        {
            let mut live = lock(&self.0.live);
            live.installing = false;
            live.step = None;
            live.message = None;
            live.percent = None;
            live.error = result.as_ref().err().map(ToString::to_string);
        }
        self.emit_settled().await;
        result
    }

    async fn install(&self, force_chromium: bool) -> Result<Launch, BrowserError> {
        if self
            .0
            .env
            .group_ready(BROWSER_PACKAGE_GROUP)
            .await
            .is_none()
        {
            if self.0.env.ready().await.is_none() {
                self.step(
                    BrowserStep::Python,
                    "Setting up Python (a one-time download)",
                );
                self.0.env.ensure().await?;
            }
            self.step(
                BrowserStep::Packages,
                "Installing the browser packages (Browser-Use, Playwright)",
            );
            self.0
                .env
                .ensure_group(BROWSER_PACKAGE_GROUP, &CHECK)
                .await?;
        }
        let settings = self.settings();
        if force_chromium && self.chromium().await.is_none() {
            self.install_chromium().await?;
        }
        match settings.source {
            BrowserSource::Cdp => Ok(Launch {
                executable: None,
                using: format!("your Chrome at {}", settings.cdp_url),
            }),
            BrowserSource::Auto if !self.installed().is_empty() => {
                let b = self.installed().remove(0);
                Ok(Launch {
                    executable: Some(b.path),
                    using: b.name,
                })
            }
            _ => {
                let path = match self.chromium().await {
                    Some(p) => p,
                    None => self.install_chromium().await?,
                };
                Ok(Launch {
                    executable: Some(path),
                    using: "Chromium".into(),
                })
            }
        }
    }

    /// Downloads Playwright's Chromium into `browsers/` and records where it is.
    async fn install_chromium(&self) -> Result<String, BrowserError> {
        self.step(
            BrowserStep::Chromium,
            "Downloading Chromium (a one-time download)",
        );
        tokio::fs::create_dir_all(self.browsers_dir())
            .await
            .map_err(|e| {
                BrowserError::Setup(format!("couldn't create the browsers folder: {e}"))
            })?;
        self.run_python_streaming(
            &["-m", "playwright", "install", "--no-shell", "chromium"],
            DOWNLOAD_LIMIT,
        )
        .await?;
        let script = "from playwright.sync_api import sync_playwright\n\
                      p = sync_playwright().start()\n\
                      print(p.chromium.executable_path)\n\
                      p.stop()\n";
        let (ok, out, err) = self.python_with_browsers(script).await?;
        let path = String::from_utf8_lossy(&out).trim().to_owned();
        if !ok || path.is_empty() || tokio::fs::metadata(&path).await.is_err() {
            return Err(BrowserError::Setup(format!(
                "Chromium was downloaded but can't be found. {}",
                String::from_utf8_lossy(&err).trim()
            )));
        }
        let marker = serde_json::to_string(&ChromiumMarker { path: path.clone() })
            .map_err(|e| BrowserError::Setup(e.to_string()))?;
        tokio::fs::write(self.chromium_marker(), marker)
            .await
            .map_err(|e| BrowserError::Setup(format!("couldn't save the browser's place: {e}")))?;
        Ok(path)
    }

    fn python_env(&self) -> Vec<(String, Option<String>)> {
        let mut env: Vec<(String, Option<String>)> = self
            .0
            .env
            .script_env()
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect();
        env.push((
            "PLAYWRIGHT_BROWSERS_PATH".into(),
            Some(self.browsers_dir().to_string_lossy().into_owned()),
        ));
        env
    }

    async fn python_with_browsers(
        &self,
        script: &str,
    ) -> Result<(bool, Vec<u8>, Vec<u8>), BrowserError> {
        let mut cmd = tokio::process::Command::new(self.0.env.venv_python());
        cmd.args(["-c", script])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (k, v) in self.python_env() {
            match v {
                Some(v) => cmd.env(k, v),
                None => cmd.env_remove(k),
            };
        }
        process::prepare(&mut cmd);
        let mut child = cmd
            .spawn()
            .map_err(|e| BrowserError::Setup(format!("Python didn't start: {e}")))?;
        let pid = child.id();
        let mut out = child.stdout.take();
        let mut err = child.stderr.take();
        let waited = tokio::time::timeout(Duration::from_secs(120), async {
            tokio::join!(
                process::capture(out.as_mut(), 1 << 20),
                process::capture(err.as_mut(), 1 << 20),
                child.wait()
            )
        })
        .await;
        let Ok((stdout, stderr, status)) = waited else {
            process::kill_tree(pid).await;
            return Err(BrowserError::Setup("Python didn't answer in time.".into()));
        };
        Ok((status.is_ok_and(|s| s.success()), stdout, stderr))
    }

    /// Runs the environment's Python with `args`, passing its output (split at `\r` and
    /// `\n`, for progress bars) to the setup log and percentages to the status.
    async fn run_python_streaming(
        &self,
        args: &[&str],
        limit: Duration,
    ) -> Result<(), BrowserError> {
        let mut cmd = tokio::process::Command::new(self.0.env.venv_python());
        cmd.args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (k, v) in self.python_env() {
            match v {
                Some(v) => cmd.env(k, v),
                None => cmd.env_remove(k),
            };
        }
        process::prepare(&mut cmd);
        let mut child = cmd
            .spawn()
            .map_err(|e| BrowserError::Setup(format!("Python didn't start: {e}")))?;
        let pid = child.id();
        let out = child.stdout.take();
        let err = child.stderr.take();
        let tail = Mutex::new(VecDeque::<String>::new());
        let waited = tokio::time::timeout(limit, async {
            let (_, _, status) =
                tokio::join!(self.pump(out, &tail), self.pump(err, &tail), child.wait());
            status
        })
        .await;
        let Ok(status) = waited else {
            process::kill_tree(pid).await;
            return Err(BrowserError::Setup(format!(
                "the download took longer than {} minutes",
                limit.as_secs() / 60
            )));
        };
        self.emit(true);
        match status {
            Ok(s) if s.success() => Ok(()),
            other => {
                let code = other
                    .ok()
                    .and_then(|s| s.code())
                    .map_or_else(|| "no exit code".to_owned(), |c| format!("exit code {c}"));
                let lines: Vec<String> = lock(&tail).iter().cloned().collect();
                Err(BrowserError::Setup(format!(
                    "the Chromium download ended with {code}.\n{}",
                    lines.join("\n")
                )))
            }
        }
    }

    async fn pump<R: tokio::io::AsyncRead + Unpin>(
        &self,
        r: Option<R>,
        tail: &Mutex<VecDeque<String>>,
    ) {
        let Some(mut r) = r else { return };
        let mut buf = [0u8; 4096];
        let mut line: Vec<u8> = Vec::new();
        loop {
            let n = match r.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            for &b in &buf[..n] {
                if b == b'\n' || b == b'\r' {
                    self.take_line(&mut line, tail);
                } else if line.len() < 4096 {
                    line.push(b);
                }
            }
        }
        self.take_line(&mut line, tail);
    }

    fn take_line(&self, line: &mut Vec<u8>, tail: &Mutex<VecDeque<String>>) {
        let text = String::from_utf8_lossy(line).trim().to_owned();
        line.clear();
        if text.is_empty() {
            return;
        }
        {
            let mut t = lock(tail);
            if t.len() >= 12 {
                t.pop_front();
            }
            t.push_back(text.clone());
        }
        let percent = percent_of(&text);
        {
            let mut live = lock(&self.0.live);
            if let Some(p) = percent {
                live.percent = Some(p);
            }
            // Progress bars repeat: keep one line per bar.
            let bar = percent.is_some();
            if bar && live.log.back().is_some_and(|l| percent_of(l).is_some()) {
                live.log.pop_back();
            }
            push(&mut live.log, text);
        }
        self.emit(false);
    }

    // ---- Servers ----------------------------------------------------------------------------

    /// Calls a tool of the chat's browser agent, setting up and starting it when needed.
    /// `call_id` is the tool call's id (progress notices carry it). `llm` is the model
    /// `browser_task` uses; a different one restarts the chat's agent.
    pub async fn call(
        &self,
        chat: &str,
        tool: &str,
        arguments: Value,
        call_id: &str,
        llm: Option<BrowserLlm>,
    ) -> Result<McpToolResult, BrowserError> {
        for attempt in 0..2 {
            let client = self.server(chat, llm.as_ref()).await?;
            match client
                .call_tool(tool, arguments.clone(), Some(call_id))
                .await
            {
                Ok(r) => {
                    self.touch(chat);
                    return Ok(r);
                }
                // The server went away (closed as idle meanwhile, or it crashed): once more.
                Err(McpError::Closed(_)) if attempt == 0 => {
                    self.drop_server(chat).await;
                }
                Err(e) => return Err(BrowserError::Call(e.to_string())),
            }
        }
        Err(BrowserError::Call(
            "the browser agent keeps stopping".into(),
        ))
    }

    fn touch(&self, chat: &str) {
        if let Some(s) = lock(&self.0.servers).get_mut(chat) {
            s.last_used = Instant::now();
        }
    }

    fn start_lock(&self, chat: &str) -> Arc<tokio::sync::Mutex<()>> {
        lock(&self.0.starting)
            .entry(chat.to_owned())
            .or_default()
            .clone()
    }

    fn server_key(settings: &BrowserSettings, launch: &Launch, llm: Option<&BrowserLlm>) -> String {
        format!(
            "{:?}|{}|{}|{}|{}|{}|{}",
            settings.source,
            settings.cdp_url,
            settings.show_window,
            settings.max_steps,
            settings.task_timeout_secs,
            launch.executable.as_deref().unwrap_or(""),
            llm.map(BrowserLlm::fingerprint).unwrap_or_default()
        )
    }

    async fn server(
        &self,
        chat: &str,
        llm: Option<&BrowserLlm>,
    ) -> Result<Arc<McpClient>, BrowserError> {
        let guard = self.start_lock(chat);
        let _one = guard.lock().await;
        let launch = self.run_setup_if_needed().await?;
        let settings = self.settings();
        let key = Self::server_key(&settings, &launch, llm);
        let existing = {
            let servers = lock(&self.0.servers);
            servers.get(chat).map(|s| (s.client.clone(), s.key == key))
        };
        match existing {
            Some((client, true)) if client.closed().borrow().is_none() => return Ok(client),
            Some(_) => self.drop_server(chat).await,
            None => {}
        }
        let client = self.spawn(chat, &settings, &launch, llm).await?;
        lock(&self.0.servers).insert(
            chat.to_owned(),
            Server {
                client: client.clone(),
                key,
                last_used: Instant::now(),
            },
        );
        self.emit(true);
        Ok(client)
    }

    /// Setup only when something is missing (it's quick to check).
    async fn run_setup_if_needed(&self) -> Result<Launch, BrowserError> {
        let settings = self.settings();
        let packages = self
            .0
            .env
            .group_ready(BROWSER_PACKAGE_GROUP)
            .await
            .is_some();
        if packages {
            match settings.source {
                BrowserSource::Cdp => {
                    return Ok(Launch {
                        executable: None,
                        using: format!("your Chrome at {}", settings.cdp_url),
                    });
                }
                BrowserSource::Auto if !self.installed().is_empty() => {
                    let b = self.installed().remove(0);
                    return Ok(Launch {
                        executable: Some(b.path),
                        using: b.name,
                    });
                }
                _ => {
                    if let Some(p) = self.chromium().await {
                        return Ok(Launch {
                            executable: Some(p),
                            using: "Chromium".into(),
                        });
                    }
                }
            }
        }
        self.run_setup(false).await
    }

    async fn write_agent(&self) -> Result<PathBuf, BrowserError> {
        let dir = self.agents_dir().join("oxide_browser_agent");
        tokio::fs::create_dir_all(&dir)
            .await
            .map_err(|e| BrowserError::Start(format!("couldn't create {}: {e}", dir.display())))?;
        for (name, text) in project::AGENT_FILES {
            let path = dir.join(name);
            let same = tokio::fs::read_to_string(&path)
                .await
                .is_ok_and(|old| old == *text);
            if !same {
                tokio::fs::write(&path, text).await.map_err(|e| {
                    BrowserError::Start(format!("couldn't write {}: {e}", path.display()))
                })?;
            }
        }
        Ok(self.agents_dir())
    }

    fn server_env(
        &self,
        settings: &BrowserSettings,
        launch: &Launch,
        llm: Option<&BrowserLlm>,
        agents: &Path,
    ) -> BTreeMap<String, String> {
        let mut env: BTreeMap<String, String> = self
            .python_env()
            .into_iter()
            .filter_map(|(k, v)| v.map(|v| (k, v)))
            .collect();
        let mut set = |k: &str, v: String| {
            env.insert(k.to_owned(), v);
        };
        set("PYTHONPATH", agents.to_string_lossy().into_owned());
        set("ANONYMIZED_TELEMETRY", "false".into());
        set("BROWSER_USE_SETUP_LOGGING", "false".into());
        let cdp = settings.source == BrowserSource::Cdp;
        set(
            "OXIDE_BROWSER_MODE",
            if cdp { "cdp" } else { "launch" }.into(),
        );
        set("OXIDE_BROWSER_CDP_URL", settings.cdp_url.clone());
        set(
            "OXIDE_BROWSER_EXECUTABLE",
            launch.executable.clone().unwrap_or_default(),
        );
        set(
            "OXIDE_BROWSER_HEADLESS",
            if settings.show_window { "0" } else { "1" }.into(),
        );
        set("OXIDE_BROWSER_MAX_STEPS", settings.max_steps.to_string());
        set(
            "OXIDE_BROWSER_TASK_TIMEOUT",
            settings.task_timeout_secs.to_string(),
        );
        if let Some(llm) = llm {
            set("OXIDE_LLM_KIND", llm.kind.as_str().into());
            set("OXIDE_LLM_BASE_URL", llm.base_url.clone());
            set("OXIDE_LLM_MODEL", llm.model.clone());
            set("OXIDE_LLM_API_KEY", llm.api_key.clone().unwrap_or_default());
            set(
                "OXIDE_LLM_VISION",
                if llm.vision { "1" } else { "0" }.into(),
            );
        }
        env
    }

    async fn spawn(
        &self,
        chat: &str,
        settings: &BrowserSettings,
        launch: &Launch,
        llm: Option<&BrowserLlm>,
    ) -> Result<Arc<McpClient>, BrowserError> {
        let agents = self.write_agent().await?;
        let env = self.server_env(settings, launch, llm, &agents);
        tracing::info!("starting the browser agent for a chat ({})", launch.using);
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let log = ServerLog::new(
            self.0.logs.as_ref().map(|l| l.join("browser-agent.log")),
            None,
        );
        let spec = ProcessSpec {
            command: self.0.env.venv_python().to_string_lossy().into_owned(),
            args: vec!["-m".into(), "oxide_browser_agent".into()],
            env,
            cwd: Some(agents),
        };
        let transport = StdioTransport::spawn(&spec, tx, log)
            .map_err(|e| BrowserError::Start(e.to_string()))?;
        let opts = ConnectOptions {
            tool_timeout: Duration::from_secs(u64::from(settings.task_timeout_secs) + 120),
            ..ConnectOptions::default()
        };
        let handler = Arc::new(FrameHandler {
            chat: chat.to_owned(),
            sink: self.0.sink.clone(),
        });
        let connect = McpClient::connect(
            BROWSER_SERVER,
            Transport::Stdio(transport),
            rx,
            handler,
            opts,
        );
        match tokio::time::timeout(CONNECT, connect).await {
            Ok(Ok(c)) => Ok(c),
            Ok(Err(e)) => Err(BrowserError::Start(e.to_string())),
            Err(_) => Err(BrowserError::Start(
                "it didn't answer within 90 seconds".into(),
            )),
        }
    }

    async fn drop_server(&self, chat: &str) {
        let server = lock(&self.0.servers).remove(chat);
        if let Some(s) = server {
            s.client.close().await;
            self.emit(true);
        }
    }

    /// The tools the chat's running agent offers (tests compare them with Oxide's list).
    #[doc(hidden)]
    pub async fn server_tools(&self, chat: &str) -> Result<Vec<String>, BrowserError> {
        let client = lock(&self.0.servers).get(chat).map(|s| s.client.clone());
        let Some(client) = client else {
            return Ok(Vec::new());
        };
        let tools = client
            .list_tools()
            .await
            .map_err(|e| BrowserError::Call(e.to_string()))?;
        Ok(tools.into_iter().map(|t| t.name).collect())
    }

    /// Closes a chat's browser (its agent stops; the next browser call starts it again).
    pub async fn close(&self, chat: &str) {
        self.drop_server(chat).await;
    }

    /// The chat was deleted.
    pub async fn forget(&self, chat: &str) {
        self.drop_server(chat).await;
        lock(&self.0.starting).remove(chat);
    }

    /// Closes browsers unused for `idle_minutes`. Returns the chats closed.
    pub async fn close_idle(&self) -> Vec<String> {
        let idle = Duration::from_secs(u64::from(self.settings().idle_minutes) * 60);
        let stale: Vec<String> = lock(&self.0.servers)
            .iter()
            .filter(|(_, s)| s.last_used.elapsed() >= idle)
            .map(|(k, _)| k.clone())
            .collect();
        for chat in &stale {
            self.drop_server(chat).await;
        }
        stale
    }

    /// Checks for idle browsers every minute (once per app).
    pub fn start_reaper(&self) {
        if self.0.reaper.swap(true, Ordering::SeqCst) {
            return;
        }
        let me = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;
                let closed = me.close_idle().await;
                if !closed.is_empty() {
                    tracing::info!("closed {} idle browser(s)", closed.len());
                }
            }
        });
    }

    /// Closes every chat's browser (Oxide is quitting).
    pub async fn shutdown(&self) {
        let all: Vec<String> = lock(&self.0.servers).keys().cloned().collect();
        let closes = all.iter().map(|c| self.drop_server(c));
        futures_util::future::join_all(closes).await;
    }

    /// Removes Oxide's Chromium (packages stay; Settings → Advanced resets those).
    pub async fn remove_chromium(&self) -> Result<(), BrowserError> {
        let _one = self.0.setup.lock().await;
        remove_dir(&self.browsers_dir())
            .await
            .map_err(|e| BrowserError::Setup(e.to_string()))?;
        self.emit_settled().await;
        Ok(())
    }
}

fn push(log: &mut VecDeque<String>, line: String) {
    if log.len() >= LOG_LINES {
        log.pop_front();
    }
    log.push_back(line);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_parse_json_and_fall_back_to_text() {
        let f = parse_frame(
            "c",
            "t",
            r#"{"text":"Browsing: Hours","url":"http://a/hours","frame":"/9j/"}"#,
        );
        assert_eq!(f.text, "Browsing: Hours");
        assert_eq!(f.url.as_deref(), Some("http://a/hours"));
        assert_eq!(f.image.as_deref(), Some("/9j/"));
        assert_eq!(
            (f.conversation_id.as_str(), f.tool_use_id.as_str()),
            ("c", "t")
        );
        let f = parse_frame("c", "t", "Working");
        assert_eq!((f.text.as_str(), f.image), ("Working", None));
        let f = parse_frame("c", "t", "{not json");
        assert_eq!(f.text, "{not json");
    }

    #[test]
    fn percentages_come_from_progress_bars() {
        assert_eq!(percent_of("|■■■■■■■■■■     |  30% of 172.9 MiB"), Some(30));
        assert_eq!(percent_of("| 100% of 1 MiB"), Some(100));
        assert_eq!(percent_of("Downloading Chromium 140.0"), None);
        assert_eq!(percent_of("x 250%"), None);
    }

    #[test]
    fn the_key_never_shows_in_debug_or_the_server_key() {
        let llm = BrowserLlm {
            kind: ProviderKind::OpenAi,
            base_url: "https://api.openai.com/v1".into(),
            model: "gpt".into(),
            api_key: Some("sk-secret-123456".into()),
            vision: true,
        };
        assert!(!format!("{llm:?}").contains("secret"));
        assert!(!llm.fingerprint().contains("secret"));
        let mut other = llm.clone();
        other.api_key = Some("sk-other".into());
        assert_ne!(llm.fingerprint(), other.fingerprint());
    }
}
