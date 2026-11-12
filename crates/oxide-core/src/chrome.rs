//! The Chrome link (Phase 14): Oxide's own Chrome extension, connected to Oxide over a
//! localhost WebSocket after pairing, drives tabs in the user's everyday Chrome.
//!
//! Plain data and pure helpers only; the bridge is `oxide-bridge`, the extension is in
//! `extension/`.

use serde::{Deserialize, Serialize};
use specta::Type;

/// Server name of the extension's tools. They reach models as `chrome__<tool>`.
pub const CHROME_SERVER: &str = "chrome";
/// `ChatTools::disabled_builtins` entry that switches Chrome off in a chat.
pub const CHROME_SWITCH: &str = "chrome";

/// Where the bridge listens by default (on 127.0.0.1 only).
pub const DEFAULT_CHROME_PORT: u16 = 47_820;
/// Lowest port Settings accepts (no privileged ports).
pub const MIN_CHROME_PORT: u16 = 1_024;

pub const CHROME_TABS_LIST: &str = "tabs_list";
pub const CHROME_TABS_CREATE: &str = "tabs_create";
pub const CHROME_TABS_SELECT: &str = "tabs_select";
pub const CHROME_TABS_CLOSE: &str = "tabs_close";
pub const CHROME_NAVIGATE: &str = "navigate";
pub const CHROME_READ_PAGE: &str = "read_page";
pub const CHROME_FIND: &str = "find";
pub const CHROME_CLICK: &str = "click";
pub const CHROME_TYPE: &str = "type";
pub const CHROME_PRESS_KEY: &str = "press_key";
pub const CHROME_SCROLL: &str = "scroll";
pub const CHROME_FORM_INPUT: &str = "form_input";
pub const CHROME_SCREENSHOT: &str = "screenshot";
pub const CHROME_GET_CONSOLE: &str = "get_console";

/// The extension's tools, in the order models get them.
pub const CHROME_TOOLS: [&str; 14] = [
    CHROME_TABS_LIST,
    CHROME_TABS_CREATE,
    CHROME_TABS_SELECT,
    CHROME_TABS_CLOSE,
    CHROME_NAVIGATE,
    CHROME_READ_PAGE,
    CHROME_FIND,
    CHROME_CLICK,
    CHROME_TYPE,
    CHROME_PRESS_KEY,
    CHROME_SCROLL,
    CHROME_FORM_INPUT,
    CHROME_SCREENSHOT,
    CHROME_GET_CONSOLE,
];

/// Whether a Chrome tool only looks (lists tabs, reads, finds, scrolls, takes a screenshot,
/// reads the console, picks which tab to work in): it changes nothing on websites, so Oxide
/// never asks for it. The extension's per-site permission still applies.
pub fn chrome_tool_reads_only(tool: &str) -> bool {
    matches!(
        tool,
        CHROME_TABS_LIST
            | CHROME_TABS_SELECT
            | CHROME_READ_PAGE
            | CHROME_FIND
            | CHROME_SCROLL
            | CHROME_SCREENSHOT
            | CHROME_GET_CONSOLE
    )
}

/// Settings → Chrome. Off by default: while on, Oxide listens for its extension.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct ChromeSettings {
    /// Listen for the extension and give models its tools while a paired Chrome is connected.
    pub enabled: bool,
    /// Port on 127.0.0.1.
    pub port: u16,
    /// Acting in Chrome (opening, clicking, typing, closing tabs) asks first. The extension
    /// asks per site as well.
    pub ask: bool,
}

impl Default for ChromeSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            port: DEFAULT_CHROME_PORT,
            ask: true,
        }
    }
}

impl ChromeSettings {
    pub fn normalized(mut self) -> Self {
        if self.port < MIN_CHROME_PORT {
            self.port = DEFAULT_CHROME_PORT;
        }
        self
    }
}

/// A Chrome that was paired with Oxide (its token is in the keychain).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PairedBrowser {
    /// Oxide's id for this pairing.
    pub id: String,
    /// What the extension calls its browser ("Chrome 142 on Windows").
    pub name: String,
    /// The extension's id (from its `chrome-extension://<id>` origin); the token works only
    /// from there.
    pub extension_id: String,
    pub paired_at: crate::Timestamp,
    pub last_seen: Option<crate::Timestamp>,
}

/// A pairing code waiting to be typed into the extension.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PairingCode {
    /// Six words separated by spaces.
    pub code: String,
    pub expires_at: crate::Timestamp,
}

/// The Chrome link as Settings shows it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ChromeStatus {
    /// Settings → Chrome is on.
    pub enabled: bool,
    /// The bridge is listening.
    pub listening: bool,
    pub port: u16,
    /// Why it couldn't listen (port taken, …), written for users.
    pub error: Option<String>,
    /// The paired browser connected now (models get the tools while one is).
    pub connected: Option<String>,
    /// The extension's version, when one is connected.
    pub extension_version: Option<String>,
    pub paired: Vec<PairedBrowser>,
    pub pairing: Option<PairingCode>,
    /// Chats working in Chrome now.
    pub sessions: Vec<String>,
    /// The folder to load unpacked in `chrome://extensions`, when Oxide knows it.
    pub extension_dir: Option<String>,
}

/// A six-word pairing code, tidied: lower case, single spaces, only letters. `None` when it
/// isn't six words.
pub fn clean_pairing_code(s: &str) -> Option<String> {
    let words: Vec<String> = s
        .split(|c: char| !c.is_ascii_alphabetic())
        .filter(|w| !w.is_empty())
        .map(str::to_ascii_lowercase)
        .collect();
    (words.len() == 6).then(|| words.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn settings_default_off_and_are_normalized() {
        let s = ChromeSettings::default();
        assert!(!s.enabled && s.ask);
        assert_eq!(s.port, DEFAULT_CHROME_PORT);
        let s = ChromeSettings {
            port: 80,
            ..ChromeSettings::default()
        }
        .normalized();
        assert_eq!(s.port, DEFAULT_CHROME_PORT);
        let back: ChromeSettings = serde_json::from_value(json!({"enabled": true})).expect("json");
        assert!(back.enabled && back.ask && back.port == DEFAULT_CHROME_PORT);
    }

    #[test]
    fn read_only_tools() {
        for t in [
            "tabs_list",
            "read_page",
            "find",
            "screenshot",
            "get_console",
        ] {
            assert!(chrome_tool_reads_only(t), "{t}");
        }
        for t in [
            "navigate",
            "click",
            "type",
            "press_key",
            "form_input",
            "tabs_close",
        ] {
            assert!(!chrome_tool_reads_only(t), "{t}");
        }
        assert_eq!(CHROME_TOOLS.len(), 14);
    }

    #[test]
    fn pairing_codes_are_tidied() {
        assert_eq!(
            clean_pairing_code("  Amber-river  TOAST, mint\nlake ocean ").as_deref(),
            Some("amber river toast mint lake ocean")
        );
        assert_eq!(clean_pairing_code("one two three"), None);
        assert_eq!(clean_pairing_code("a b c d e f g"), None);
    }
}
