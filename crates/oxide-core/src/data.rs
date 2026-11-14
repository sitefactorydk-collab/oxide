//! Settings → Data (export everything, delete everything) and app updates (Phase 17).

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::Timestamp;

/// The word a user types to confirm "Delete all data".
pub const DELETE_ALL_CONFIRMATION: &str = "delete";

/// Where Oxide keeps its files (Settings → Data).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DataLocations {
    /// The database, attachments, skills, extensions, the managed Python, sandboxes.
    pub data: String,
    /// The MCP config file and window positions.
    pub config: String,
    /// MCP server and sandbox logs.
    pub logs: String,
    /// Bytes used by the data folder, when it could be measured.
    pub data_bytes: Option<u64>,
}

/// What "Export all" wrote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DataExport {
    /// The zip file.
    pub path: String,
    /// Database tables written (one JSON file each).
    pub tables: u32,
    /// Rows across those tables.
    pub rows: u64,
    /// Files copied in (attachments, skills, extension manifests, the MCP config).
    pub files: u32,
    /// Size of the zip.
    pub bytes: u64,
    /// Files left out (too large or unreadable), with the reason.
    pub skipped: Vec<String>,
}

/// What "Delete all data" did before Oxide restarts.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DataDeleted {
    /// Keychain entries removed (API keys, sign-ins, pairings, extension secrets).
    pub secrets: u32,
    /// Files and folders that couldn't be removed yet; Oxide tries again when it starts.
    pub leftovers: Vec<String>,
}

/// Settings → About → Updates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct UpdateSettings {
    /// Look for a new version shortly after start and then every few hours.
    pub check_automatically: bool,
}

impl Default for UpdateSettings {
    fn default() -> Self {
        Self {
            check_automatically: true,
        }
    }
}

/// A newer version on the update server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    /// Release date as the update manifest gives it (RFC 3339), if any.
    pub date: Option<String>,
    /// Release notes from the manifest (plain text or Markdown).
    pub notes: Option<String>,
}

/// Where the updater is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum UpdatePhase {
    /// Nothing checked yet in this run.
    Idle,
    Checking,
    UpToDate,
    /// `available` says which version.
    Available,
    /// `downloaded` / `total` say how far.
    Downloading,
    /// Installed; Oxide restarts (Windows' installer closes Oxide itself).
    Installing,
    /// `error` says why.
    Failed,
}

/// The updater's state (Settings → About, and the `updateChanged` event).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    pub current_version: String,
    /// This build knows where to look for updates (release builds made by CI).
    pub configured: bool,
    /// Updates can be installed from inside Oxide. False for Linux `.deb`/`.rpm` installs,
    /// which the system's package manager updates; `note` says so.
    pub can_install: bool,
    pub phase: UpdatePhase,
    pub available: Option<UpdateInfo>,
    pub downloaded: u64,
    pub total: Option<u64>,
    /// Why the last check or install failed, written for users.
    pub error: Option<String>,
    pub checked_at: Option<Timestamp>,
    /// Something users should know about updates on this install, if anything.
    pub note: Option<String>,
}

impl UpdateStatus {
    pub fn new(current_version: impl Into<String>, configured: bool, can_install: bool) -> Self {
        Self {
            current_version: current_version.into(),
            configured,
            can_install,
            phase: UpdatePhase::Idle,
            available: None,
            downloaded: 0,
            total: None,
            error: None,
            checked_at: None,
            note: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_settings_default_to_checking() {
        let s: UpdateSettings = serde_json::from_str("{}").expect("parses");
        assert!(s.check_automatically);
        let s: crate::AppSettings = serde_json::from_str("{}").expect("parses");
        assert!(s.updates.check_automatically);
    }

    #[test]
    fn update_status_serializes_camel_case() {
        let mut s = UpdateStatus::new("0.1.0", true, true);
        s.phase = UpdatePhase::Available;
        s.available = Some(UpdateInfo {
            version: "0.2.0".into(),
            date: None,
            notes: Some("Fixes".into()),
        });
        let json = serde_json::to_value(&s).expect("serializes");
        assert_eq!(json["currentVersion"], "0.1.0");
        assert_eq!(json["phase"], "available");
        assert_eq!(json["available"]["version"], "0.2.0");
        assert_eq!(json["canInstall"], true);
    }
}
