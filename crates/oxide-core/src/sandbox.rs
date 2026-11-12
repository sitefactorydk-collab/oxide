//! The Docker sandbox (Phase 12b): a container per chat where models can build, run and
//! test code without touching the computer, with its files served by the reference
//! filesystem MCP server.
//!
//! Plain data and pure helpers only; the Docker work is in `oxide-sandbox`.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::Timestamp;

/// Runs a command in the chat's sandbox.
pub const SANDBOX_EXEC: &str = "sandbox_exec";
/// Copies a file or folder from a shared folder into the sandbox.
pub const SANDBOX_COPY_IN: &str = "sandbox_copy_in";
/// Copies a file or folder from the sandbox into a shared folder.
pub const SANDBOX_COPY_OUT: &str = "sandbox_copy_out";
/// The sandbox's built-in tools, in the order models get them.
pub const SANDBOX_TOOLS: [&str; 3] = [SANDBOX_EXEC, SANDBOX_COPY_IN, SANDBOX_COPY_OUT];

/// Server name of the sandbox's file tools (the filesystem MCP server). Its tools reach
/// models as `sandbox__<tool>`; it is Oxide's own, not an entry in `oxide_config.json`.
pub const SANDBOX_SERVER: &str = "sandbox";
/// `ChatTools::disabled_builtins` entry that switches the whole sandbox off in a chat.
pub const SANDBOX_SWITCH: &str = "sandbox";

/// Where a chat's workspace is mounted inside its containers.
pub const WORKSPACE_DIR: &str = "/workspace";

/// The default image commands run in: Debian with Python 3.12, pip, gcc/g++, make, git and
/// curl (the full `python` image is built on `buildpack-deps`).
pub const DEFAULT_SANDBOX_IMAGE: &str = "python:3.12-bookworm";
/// The reference filesystem MCP server's image.
pub const DEFAULT_FILES_IMAGE: &str = "mcp/filesystem";
/// Where Docker is explained and downloaded.
pub const DOCKER_INSTALL_URL: &str = "https://docs.docker.com/get-started/get-docker/";

/// Default and largest time limit of one command, in seconds.
pub const DEFAULT_SANDBOX_TIMEOUT_SECS: u32 = 600;
pub const MAX_SANDBOX_TIMEOUT_SECS: u32 = 3_600;
/// Default CPUs and memory of a sandbox.
pub const DEFAULT_SANDBOX_CPUS: u32 = 2;
pub const MAX_SANDBOX_CPUS: u32 = 64;
pub const DEFAULT_SANDBOX_MEMORY_MB: u32 = 2_048;
pub const MIN_SANDBOX_MEMORY_MB: u32 = 256;
pub const MAX_SANDBOX_MEMORY_MB: u32 = 262_144;
/// Default minutes without use after which a sandbox is stopped (its files stay).
pub const DEFAULT_SANDBOX_IDLE_MINUTES: u32 = 15;
pub const MAX_SANDBOX_IDLE_MINUTES: u32 = 1_440;
/// Most entries one listing returns.
pub const MAX_LISTING: usize = 500;

/// Where workspaces live.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum SandboxStorage {
    /// A Docker volume per chat (inside Docker's own disk).
    #[default]
    Volume,
    /// A folder per chat on this computer (`SandboxSettings::folder`, or Oxide's data
    /// folder), mounted into the container. Only that folder is mounted.
    Folder,
}

/// Settings → Sandbox. Off by default: it needs Docker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct SandboxSettings {
    /// Models get the sandbox's tools (when Docker runs).
    pub enabled: bool,
    /// The image commands run in.
    pub image: String,
    /// The filesystem MCP server's image.
    pub files_image: String,
    pub cpus: u32,
    pub memory_mb: u32,
    /// The sandbox can reach the internet (to install packages). Off: no network at all.
    pub network: bool,
    pub storage: SandboxStorage,
    /// With `Folder` storage: the folder that holds one folder per chat. Empty: Oxide's data
    /// folder.
    pub folder: String,
    /// Minutes without use before a sandbox is stopped.
    pub idle_minutes: u32,
    /// Time limit of one command, in seconds.
    pub timeout_secs: u32,
    /// Commands and file changes inside the sandbox ask first (like connector tools).
    /// Copies in and out of the sandbox always ask.
    pub ask: bool,
}

impl Default for SandboxSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            image: DEFAULT_SANDBOX_IMAGE.into(),
            files_image: DEFAULT_FILES_IMAGE.into(),
            cpus: DEFAULT_SANDBOX_CPUS,
            memory_mb: DEFAULT_SANDBOX_MEMORY_MB,
            network: false,
            storage: SandboxStorage::Volume,
            folder: String::new(),
            idle_minutes: DEFAULT_SANDBOX_IDLE_MINUTES,
            timeout_secs: DEFAULT_SANDBOX_TIMEOUT_SECS,
            ask: true,
        }
    }
}

impl SandboxSettings {
    /// Clamps numbers and falls back to the default images for names Docker wouldn't take.
    pub fn normalized(mut self) -> Self {
        self.image = clean_image(&self.image).unwrap_or_else(|| DEFAULT_SANDBOX_IMAGE.into());
        self.files_image =
            clean_image(&self.files_image).unwrap_or_else(|| DEFAULT_FILES_IMAGE.into());
        self.cpus = self.cpus.clamp(1, MAX_SANDBOX_CPUS);
        self.memory_mb = self
            .memory_mb
            .clamp(MIN_SANDBOX_MEMORY_MB, MAX_SANDBOX_MEMORY_MB);
        self.idle_minutes = self.idle_minutes.clamp(1, MAX_SANDBOX_IDLE_MINUTES);
        self.timeout_secs = self.timeout_secs.clamp(1, MAX_SANDBOX_TIMEOUT_SECS);
        self.folder = self.folder.trim().to_owned();
        self
    }

    /// What a container is made with; a container made with other values is replaced (its
    /// workspace stays) the next time it is used.
    pub fn container_key(&self) -> String {
        format!(
            "{}|{}|{}|{}",
            self.image,
            self.cpus,
            self.memory_mb,
            if self.network { "net" } else { "nonet" }
        )
    }
}

/// An image name Docker accepts (`name[:tag][@digest]`, registry and path allowed), trimmed.
pub fn clean_image(s: &str) -> Option<String> {
    let s = s.trim();
    let ok = !s.is_empty()
        && s.len() <= 255
        && !s.starts_with(['-', '.', '/', ':', '@'])
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/' | ':' | '@'));
    ok.then(|| s.to_owned())
}

/// The name of a chat's container and volume (`oxide-sandbox-<chat id>`), and with
/// `files` its filesystem server's container. Docker names allow `[a-zA-Z0-9_.-]`.
pub fn sandbox_name(conversation_id: &str, files: bool) -> String {
    let id: String = conversation_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
        .take(64)
        .collect();
    let id = if id.is_empty() { "chat".into() } else { id };
    if files {
        format!("oxide-sandbox-files-{id}")
    } else {
        format!("oxide-sandbox-{id}")
    }
}

/// A path inside the workspace, as models write it (`src/app.py`, `/workspace/src`,
/// `./x/../y`), made absolute and normalized. Paths that leave `/workspace` are refused.
pub fn workspace_path(raw: &str) -> Result<String, String> {
    let raw = raw.trim().replace('\\', "/");
    let rel = match raw.strip_prefix(WORKSPACE_DIR) {
        Some("") => "",
        Some(rest) if rest.starts_with('/') => rest,
        _ if raw.starts_with('/') => {
            return Err(format!(
                "“{raw}” is outside the sandbox's workspace; use a path under {WORKSPACE_DIR}."
            ));
        }
        _ => raw.as_str(),
    };
    let mut parts: Vec<&str> = Vec::new();
    for p in rel.split('/') {
        match p {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return Err(format!(
                        "“{raw}” leaves the sandbox's workspace ({WORKSPACE_DIR})."
                    ));
                }
            }
            p => parts.push(p),
        }
    }
    if parts.is_empty() {
        Ok(WORKSPACE_DIR.to_owned())
    } else {
        Ok(format!("{WORKSPACE_DIR}/{}", parts.join("/")))
    }
}

/// The last part of a workspace path (`/workspace/a/b` → `b`; the workspace itself →
/// `workspace`).
pub fn base_name(path: &str) -> &str {
    path.trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|n| !n.is_empty())
        .unwrap_or("workspace")
}

/// Whether Docker can be used.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum DockerState {
    /// Not checked yet.
    #[default]
    Unknown,
    /// The `docker` program isn't installed (or not where Oxide looks).
    Missing,
    /// Installed, but its engine doesn't answer (Docker Desktop isn't running).
    Stopped,
    Ready,
}

/// An image the sandbox uses, and whether it is downloaded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SandboxImage {
    pub name: String,
    /// What it is for ("Commands", "Files").
    pub role: String,
    pub present: bool,
    pub size_bytes: Option<u64>,
}

/// Docker and the sandbox's images (Settings → Sandbox).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DockerStatus {
    pub state: DockerState,
    /// The engine's version (when it runs).
    pub version: Option<String>,
    /// Why it can't be used, written for users.
    pub message: Option<String>,
    /// The `docker` program Oxide found.
    pub program: Option<String>,
    /// CPUs and memory the engine has (a sandbox's limits are capped to them).
    pub cpus: Option<u32>,
    pub memory_bytes: Option<u64>,
    pub images: Vec<SandboxImage>,
    /// An image is being downloaded (its name).
    pub pulling: Option<String>,
    pub install_url: String,
    pub checked_at: Timestamp,
}

/// Whether a chat's sandbox exists and runs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum SandboxState {
    /// Never used (or reset): nothing exists yet.
    #[default]
    None,
    /// Being created (the image may be downloading).
    Starting,
    /// Stopped (idle); its files are kept and it starts again when used.
    Stopped,
    Running,
}

/// A command running in a sandbox.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SandboxCommand {
    pub id: String,
    pub command: String,
    pub started_at: Timestamp,
}

/// One chat's sandbox (the session panel's Sandbox tab).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ChatSandbox {
    pub conversation_id: String,
    pub state: SandboxState,
    /// The container's name.
    pub container: String,
    /// The image it was made from.
    pub image: Option<String>,
    /// Where its files live: a Docker volume's name or a folder on this computer.
    pub workspace: Option<String>,
    pub storage: SandboxStorage,
    pub network: bool,
    /// Commands running now.
    pub commands: Vec<SandboxCommand>,
    /// Why it couldn't start, written for users.
    pub error: Option<String>,
}

/// A file or folder in a sandbox's workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SandboxFile {
    pub name: String,
    /// The full path inside the sandbox (`/workspace/...`).
    pub path: String,
    pub dir: bool,
    pub link: bool,
    pub size: Option<u64>,
}

/// The contents of one folder in a sandbox.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SandboxListing {
    pub path: String,
    pub entries: Vec<SandboxFile>,
    pub truncated: bool,
}

/// Something about the sandbox changed (sent as `events.sandboxChanged`): Docker or the images
/// (`conversation_id` is `None`), or one chat's sandbox.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SandboxChange {
    pub conversation_id: Option<String>,
}

/// The first line of a command, shortened, for the usage list and tool lines.
pub fn command_label(command: &str) -> String {
    let line = command
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    let mut s: String = line.chars().take(80).collect();
    if line.chars().count() > 80 {
        s.push('…');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_default_off_without_network_and_are_clamped() {
        let s = SandboxSettings::default();
        assert!(!s.enabled);
        assert!(!s.network, "the network is off by default");
        assert!(s.ask);
        assert_eq!(s.image, DEFAULT_SANDBOX_IMAGE);
        let s = SandboxSettings {
            image: "  bad image!  ".into(),
            files_image: " ghcr.io/x/fs:1.2 ".into(),
            cpus: 0,
            memory_mb: 1,
            idle_minutes: 0,
            timeout_secs: 99_999,
            ..SandboxSettings::default()
        }
        .normalized();
        assert_eq!(s.image, DEFAULT_SANDBOX_IMAGE);
        assert_eq!(s.files_image, "ghcr.io/x/fs:1.2");
        assert_eq!(s.cpus, 1);
        assert_eq!(s.memory_mb, MIN_SANDBOX_MEMORY_MB);
        assert_eq!(s.idle_minutes, 1);
        assert_eq!(s.timeout_secs, MAX_SANDBOX_TIMEOUT_SECS);
    }

    #[test]
    fn images_are_checked() {
        assert_eq!(clean_image("node:22"), Some("node:22".into()));
        assert!(clean_image("a@sha256:abc").is_some());
        assert_eq!(clean_image("--privileged"), None);
        assert_eq!(clean_image("x y"), None);
        assert_eq!(clean_image(""), None);
        assert_eq!(clean_image("a;rm"), None);
    }

    #[test]
    fn names_are_docker_safe() {
        assert_eq!(sandbox_name("019a-bc", false), "oxide-sandbox-019a-bc");
        assert_eq!(sandbox_name("019a-bc", true), "oxide-sandbox-files-019a-bc");
        assert_eq!(sandbox_name("a/b c;d", false), "oxide-sandbox-abcd");
        assert_eq!(sandbox_name("///", false), "oxide-sandbox-chat");
    }

    #[test]
    fn workspace_paths_stay_inside() {
        assert_eq!(workspace_path("").as_deref(), Ok("/workspace"));
        assert_eq!(
            workspace_path("src/app.py").as_deref(),
            Ok("/workspace/src/app.py")
        );
        assert_eq!(workspace_path("/workspace/").as_deref(), Ok("/workspace"));
        assert_eq!(
            workspace_path("/workspace/a/./b/../c").as_deref(),
            Ok("/workspace/a/c")
        );
        assert_eq!(workspace_path(r"a\b").as_deref(), Ok("/workspace/a/b"));
        assert!(workspace_path("../etc/passwd").is_err());
        assert!(workspace_path("/workspace/../etc").is_err());
        assert!(workspace_path("/etc/passwd").is_err());
        assert!(workspace_path("/workspacex/a").is_err());
        assert_eq!(base_name("/workspace/a/b/"), "b");
        assert_eq!(base_name("/workspace"), "workspace");
    }

    #[test]
    fn container_key_changes_with_what_a_container_is_made_with() {
        let a = SandboxSettings::default();
        let mut b = a.clone();
        b.timeout_secs = 5;
        b.ask = false;
        assert_eq!(a.container_key(), b.container_key());
        b.network = true;
        assert_ne!(a.container_key(), b.container_key());
    }

    #[test]
    fn command_labels_are_short() {
        assert_eq!(command_label("\n  make test \nmore"), "make test");
        assert_eq!(command_label(&"x".repeat(100)).chars().count(), 81);
    }
}
