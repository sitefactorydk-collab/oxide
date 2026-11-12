//! The browser agent (Phase 13): an MCP server in Oxide's managed Python environment that
//! drives a Chromium browser, with Browser-Use for whole browsing tasks.
//!
//! Plain data and pure helpers only; the processes are in `oxide-pyenv`.

use serde::{Deserialize, Serialize};
use specta::Type;

/// Server name of the browser's tools. They reach models as `browser__<tool>`; the server is
/// Oxide's own (one per chat), not an entry in `oxide_config.json`.
pub const BROWSER_SERVER: &str = "browser";
/// `ChatTools::disabled_builtins` entry that switches the browser off in a chat.
pub const BROWSER_SWITCH: &str = "browser";

/// Hands a whole goal to Browser-Use.
pub const BROWSER_TASK: &str = "browser_task";
pub const BROWSER_NAVIGATE: &str = "navigate";
/// Searches Google in the browser (so models find real addresses instead of guessing).
pub const BROWSER_SEARCH: &str = "search";
pub const BROWSER_READ_PAGE: &str = "read_page";
pub const BROWSER_CLICK: &str = "click";
pub const BROWSER_TYPE: &str = "type";
pub const BROWSER_SCREENSHOT: &str = "screenshot";
pub const BROWSER_CLOSE: &str = "close";
/// The server's tools, in the order models get them.
pub const BROWSER_TOOLS: [&str; 8] = [
    BROWSER_TASK,
    BROWSER_NAVIGATE,
    BROWSER_SEARCH,
    BROWSER_READ_PAGE,
    BROWSER_CLICK,
    BROWSER_TYPE,
    BROWSER_SCREENSHOT,
    BROWSER_CLOSE,
];

/// Whether a browser tool only looks (reads the page, takes a screenshot, searches Google):
/// it doesn't act on websites, so it never asks.
pub fn browser_tool_reads_only(tool: &str) -> bool {
    matches!(
        tool,
        BROWSER_READ_PAGE | BROWSER_SCREENSHOT | BROWSER_SEARCH
    )
}

/// Where Chrome listens when the user starts it with `--remote-debugging-port=9222`.
pub const DEFAULT_CDP_URL: &str = "http://127.0.0.1:9222";
/// The dependency group of `python/pyproject.toml` the browser agent needs.
pub const BROWSER_PACKAGE_GROUP: &str = "browser";
/// Modules that must import for the browser agent to run.
pub const BROWSER_MODULES: [&str; 3] = ["mcp", "playwright", "browser_use"];

pub const DEFAULT_BROWSER_MAX_STEPS: u32 = 25;
pub const MAX_BROWSER_MAX_STEPS: u32 = 200;
pub const DEFAULT_BROWSER_TASK_TIMEOUT_SECS: u32 = 600;
pub const MIN_BROWSER_TASK_TIMEOUT_SECS: u32 = 10;
pub const MAX_BROWSER_TASK_TIMEOUT_SECS: u32 = 3_600;
pub const DEFAULT_BROWSER_IDLE_MINUTES: u32 = 15;
pub const MAX_BROWSER_IDLE_MINUTES: u32 = 1_440;

/// Which browser the agent drives.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum BrowserSource {
    /// Chrome or Edge installed on this computer (with a profile of Oxide's own), else the
    /// Chromium Oxide downloads.
    #[default]
    Auto,
    /// Always the Chromium Oxide downloads (Playwright's build).
    Bundled,
    /// A Chrome the user started with remote debugging, reached at `cdp_url` (their tabs
    /// and sign-ins; Oxide works in a tab of its own).
    Cdp,
}

/// Settings → Browser. Off by default: it installs extra packages and a browser.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct BrowserSettings {
    /// Models get the browser's tools.
    pub enabled: bool,
    pub source: BrowserSource,
    /// With `Cdp`: where the running Chrome listens.
    pub cdp_url: String,
    /// Show the browser window (otherwise it runs headless; the chat shows screenshots).
    pub show_window: bool,
    /// Browsing actions (open, click, type, tasks) ask first. Reading the page and
    /// screenshots never ask.
    pub ask: bool,
    /// Most steps of one `browser_task`.
    pub max_steps: u32,
    /// Time limit of one `browser_task`, in seconds.
    pub task_timeout_secs: u32,
    /// Minutes without use before a chat's browser is closed.
    pub idle_minutes: u32,
    /// Model for `browser_task` (`provider:model`); `None`: the chat's own model.
    pub agent_model: Option<String>,
}

impl Default for BrowserSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            source: BrowserSource::Auto,
            cdp_url: DEFAULT_CDP_URL.into(),
            show_window: false,
            ask: true,
            max_steps: DEFAULT_BROWSER_MAX_STEPS,
            task_timeout_secs: DEFAULT_BROWSER_TASK_TIMEOUT_SECS,
            idle_minutes: DEFAULT_BROWSER_IDLE_MINUTES,
            agent_model: None,
        }
    }
}

impl BrowserSettings {
    pub fn normalized(mut self) -> Self {
        self.cdp_url = clean_cdp_url(&self.cdp_url).unwrap_or_else(|| DEFAULT_CDP_URL.into());
        self.max_steps = self.max_steps.clamp(1, MAX_BROWSER_MAX_STEPS);
        self.task_timeout_secs = self
            .task_timeout_secs
            .clamp(MIN_BROWSER_TASK_TIMEOUT_SECS, MAX_BROWSER_TASK_TIMEOUT_SECS);
        self.idle_minutes = self.idle_minutes.clamp(1, MAX_BROWSER_IDLE_MINUTES);
        self.agent_model = self
            .agent_model
            .map(|m| m.trim().to_owned())
            .filter(|m| crate::model::split_model_id(m).is_some());
        self
    }
}

/// A CDP address the agent may connect to: `http(s)://` or `ws(s)://` on this computer
/// only (a remote debugging port elsewhere would hand the browser to another machine).
pub fn clean_cdp_url(s: &str) -> Option<String> {
    let s = s.trim().trim_end_matches('/');
    let rest = ["http://", "https://", "ws://", "wss://"]
        .iter()
        .find_map(|p| s.strip_prefix(p))?;
    let host_port = rest.split('/').next().unwrap_or("");
    let host = if let Some(h) = host_port.strip_prefix('[') {
        h.split(']').next().unwrap_or("")
    } else {
        host_port.split(':').next().unwrap_or("")
    };
    matches!(host, "127.0.0.1" | "localhost" | "::1").then(|| s.to_owned())
}

/// Steps of setting the browser up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum BrowserStep {
    /// The managed Python environment (Phase 12).
    Python,
    /// The browser packages (Browser-Use, MCP SDK, Playwright).
    Packages,
    /// Playwright's Chromium (a one-time download).
    Chromium,
}

/// A browser found on this computer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InstalledBrowser {
    /// "Google Chrome", "Microsoft Edge", "Chromium".
    pub name: String,
    pub path: String,
}

/// The browser agent as Settings shows it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BrowserStatus {
    /// The browser packages are installed in the managed environment.
    pub packages: bool,
    /// Versions of what is installed (`browser-use`, `playwright`, `mcp`).
    pub versions: Vec<crate::PythonPackage>,
    /// The Chromium Oxide downloaded, when it has.
    pub chromium: Option<String>,
    /// Chrome, Edge or Chromium installed on this computer.
    pub installed: Vec<InstalledBrowser>,
    /// What a new chat's browser will be (e.g. "Microsoft Edge"), or `None` when it still
    /// has to be downloaded.
    pub using: Option<String>,
    pub installing: bool,
    pub step: Option<BrowserStep>,
    pub message: Option<String>,
    /// Download progress of the current step, 0–100, when known.
    pub percent: Option<u32>,
    /// Why the last setup failed, written for users.
    pub error: Option<String>,
    /// The last lines of the setup's output.
    pub log: Vec<String>,
    /// Chats whose browser is open now.
    pub open_chats: Vec<String>,
    /// Where Oxide keeps its own Chromium.
    pub browsers_dir: String,
}

/// A live view of the browser while a call runs (a screenshot and where it is).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BrowserFrame {
    pub conversation_id: String,
    pub tool_use_id: String,
    /// What it is doing ("Browsing: Opening hours – The Little Bakery").
    pub text: String,
    pub url: Option<String>,
    /// A JPEG screenshot, base64.
    pub image: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn settings_default_off_and_are_normalized() {
        let s = BrowserSettings::default();
        assert!(!s.enabled && s.ask && !s.show_window);
        assert_eq!(s.source, BrowserSource::Auto);
        let s = BrowserSettings {
            cdp_url: "http://example.com:9222".into(),
            max_steps: 0,
            task_timeout_secs: 1,
            idle_minutes: 0,
            agent_model: Some("nonsense".into()),
            ..BrowserSettings::default()
        }
        .normalized();
        assert_eq!(s.cdp_url, DEFAULT_CDP_URL);
        assert_eq!(s.max_steps, 1);
        assert_eq!(s.task_timeout_secs, MIN_BROWSER_TASK_TIMEOUT_SECS);
        assert_eq!(s.idle_minutes, 1);
        assert_eq!(s.agent_model, None);
        let back: BrowserSettings = serde_json::from_value(json!({"enabled": true})).expect("json");
        assert!(back.enabled && back.ask);
    }

    #[test]
    fn cdp_urls_stay_on_this_computer() {
        assert_eq!(
            clean_cdp_url(" http://127.0.0.1:9333/ ").as_deref(),
            Some("http://127.0.0.1:9333")
        );
        assert!(clean_cdp_url("ws://localhost:9222/devtools/browser/abc").is_some());
        assert!(clean_cdp_url("http://[::1]:9222").is_some());
        assert_eq!(clean_cdp_url("http://10.0.0.2:9222"), None);
        assert_eq!(clean_cdp_url("http://127.0.0.1.evil.com:9222"), None);
        assert_eq!(clean_cdp_url("file:///x"), None);
    }

    #[test]
    fn read_only_tools() {
        assert!(browser_tool_reads_only("read_page") && !browser_tool_reads_only("click"));
        assert!(browser_tool_reads_only("search") && !browser_tool_reads_only("navigate"));
    }
}
