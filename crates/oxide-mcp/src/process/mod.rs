//! Starting local server processes: finding the program, building its environment, hiding
//! its console window and making sure its whole process tree goes away with it.

#[cfg(windows)]
mod windows;

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use crate::McpError;

/// What to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessSpec {
    pub command: String,
    pub args: Vec<String>,
    /// Extra variables, merged over Oxide's own environment.
    pub env: BTreeMap<String, String>,
    pub cwd: Option<PathBuf>,
}

/// Folders where Node, uv and friends usually live, added to `PATH` (after the user's own
/// entries) because GUI apps often start with a shorter `PATH` than a terminal.
fn extra_dirs() -> Vec<PathBuf> {
    let home = home_dir();
    let mut dirs: Vec<PathBuf> = Vec::new();
    if cfg!(windows) {
        for var in ["ProgramFiles", "ProgramFiles(x86)"] {
            if let Some(p) = std::env::var_os(var) {
                dirs.push(PathBuf::from(p).join("nodejs"));
            }
        }
        if let Some(p) = std::env::var_os("APPDATA") {
            dirs.push(PathBuf::from(p).join("npm"));
        }
        if let Some(p) = std::env::var_os("LOCALAPPDATA") {
            dirs.push(PathBuf::from(&p).join("Programs").join("uv"));
            dirs.push(PathBuf::from(p).join("Volta").join("bin"));
        }
    } else {
        dirs.push("/opt/homebrew/bin".into());
        dirs.push("/usr/local/bin".into());
    }
    if let Some(h) = home {
        dirs.push(h.join(".local").join("bin"));
        dirs.push(h.join(".cargo").join("bin"));
        dirs.push(h.join(".volta").join("bin"));
        dirs.push(h.join(".bun").join("bin"));
        dirs.push(h.join(".deno").join("bin"));
    }
    dirs
}

/// How much memory Oxide's own process uses (resident set / working set), in bytes, where
/// the system says. The web view's processes aren't included.
pub fn own_memory_bytes() -> Option<u64> {
    #[cfg(windows)]
    {
        windows::own_working_set()
    }
    #[cfg(target_os = "linux")]
    {
        let status = std::fs::read_to_string("/proc/self/status").ok()?;
        let line = status.lines().find(|l| l.starts_with("VmRSS:"))?;
        let kb: u64 = line
            .trim_start_matches("VmRSS:")
            .trim()
            .trim_end_matches("kB")
            .trim()
            .parse()
            .ok()?;
        Some(kb * 1024)
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        None
    }
}

pub fn home_dir() -> Option<PathBuf> {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(var)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// The `PATH` a server gets: its own `PATH` from the config if set, else Oxide's, followed by
/// the usual install folders that exist and aren't listed yet.
pub fn child_path(env: &BTreeMap<String, String>) -> OsString {
    let base: OsString = env
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("PATH"))
        .map(|(_, v)| OsString::from(v))
        .or_else(|| std::env::var_os("PATH"))
        .unwrap_or_default();
    let mut dirs: Vec<PathBuf> = std::env::split_paths(&base).collect();
    for d in extra_dirs() {
        if d.is_dir() && !dirs.contains(&d) {
            dirs.push(d);
        }
    }
    std::env::join_paths(dirs).unwrap_or(base)
}

/// Executable extensions to try on Windows (`PATHEXT`, or its usual value).
fn path_exts() -> Vec<String> {
    let raw = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into());
    raw.split(';')
        .filter(|e| !e.is_empty())
        .map(|e| e.to_ascii_lowercase())
        .collect()
}

fn is_executable(p: &Path, windows: bool) -> bool {
    if !p.is_file() {
        return false;
    }
    if windows {
        return true;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        p.metadata()
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// Candidates for `name` in `dir`. On Windows a bare `npx` must resolve to `npx.cmd`, never
/// to the extensionless shell script npm installs next to it.
fn candidates(dir: &Path, name: &str, windows: bool, exts: &[String]) -> Vec<PathBuf> {
    if !windows {
        return vec![dir.join(name)];
    }
    let lower = name.to_ascii_lowercase();
    if exts.iter().any(|e| lower.ends_with(e.as_str())) {
        vec![dir.join(name)]
    } else {
        exts.iter()
            .map(|e| dir.join(format!("{name}{e}")))
            .collect()
    }
}

/// Finds the program to run, searching `path` like a shell would.
pub fn resolve_command(command: &str, path: &OsStr) -> Result<PathBuf, McpError> {
    resolve_with(command, path, cfg!(windows), &path_exts())
}

fn resolve_with(
    command: &str,
    path: &OsStr,
    windows: bool,
    exts: &[String],
) -> Result<PathBuf, McpError> {
    let command = command.trim();
    if command.is_empty() {
        return Err(McpError::Config("the command is empty".into()));
    }
    let has_sep = command.contains('/') || (windows && command.contains('\\'));
    if has_sep {
        let p = PathBuf::from(command);
        let dir = p.parent().map(Path::to_path_buf).unwrap_or_default();
        let name = p
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        return candidates(&dir, &name, windows, exts)
            .into_iter()
            .find(|c| is_executable(c, windows))
            .ok_or_else(|| McpError::Spawn(format!("\"{command}\" doesn't exist")));
    }
    for dir in std::env::split_paths(path) {
        if let Some(found) = candidates(&dir, command, windows, exts)
            .into_iter()
            .find(|c| is_executable(c, windows))
        {
            return Ok(found);
        }
    }
    Err(McpError::Spawn(not_found_hint(command)))
}

/// A helpful message for a program that isn't installed.
fn not_found_hint(command: &str) -> String {
    let base = command
        .trim_end_matches(".exe")
        .trim_end_matches(".cmd")
        .to_ascii_lowercase();
    match base.as_str() {
        "npx" | "node" | "npm" => format!(
            "Couldn't find \"{command}\". Install Node.js from https://nodejs.org (it includes \
             npx), then restart Oxide."
        ),
        "uvx" | "uv" => format!(
            "Couldn't find \"{command}\". Install uv from https://docs.astral.sh/uv/, then \
             restart Oxide."
        ),
        "python" | "python3" | "py" => format!(
            "Couldn't find \"{command}\". Install Python, or run the server with uvx instead."
        ),
        "docker" => {
            format!("Couldn't find \"{command}\". Install Docker Desktop, then restart Oxide.")
        }
        _ => format!("Couldn't find \"{command}\" on the PATH."),
    }
}

/// A started server process with its pipes.
pub struct Spawned {
    pub child: tokio::process::Child,
    pub pid: Option<u32>,
    pub tree: ProcessTree,
}

/// Kills a server together with everything it started. On Windows a Job Object ends the
/// whole tree; elsewhere only the direct child is killed (signalling a process group from
/// here proved risky: a wrong group id takes down whatever started Oxide).
pub struct ProcessTree {
    #[cfg(windows)]
    job: Option<windows::Job>,
}

impl ProcessTree {
    /// Ends the whole tree (best effort). The direct child is killed separately.
    pub fn kill(&self) {
        #[cfg(windows)]
        if let Some(job) = &self.job {
            job.terminate();
        }
    }
}

/// Starts `spec` with piped stdio, no console window, and its tree tied to Oxide.
pub fn spawn(spec: &ProcessSpec) -> Result<Spawned, McpError> {
    let path = child_path(&spec.env);
    let program = resolve_command(&spec.command, &path)?;
    let mut cmd = tokio::process::Command::new(&program);
    cmd.args(&spec.args)
        .env("PATH", &path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    for (k, v) in &spec.env {
        if !k.eq_ignore_ascii_case("PATH") {
            cmd.env(k, v);
        }
    }
    if let Some(dir) = spec.cwd.as_ref().filter(|d| d.is_dir()) {
        cmd.current_dir(dir);
    }
    #[cfg(windows)]
    cmd.creation_flags(windows::CREATE_NO_WINDOW);

    let child = cmd
        .spawn()
        .map_err(|e| McpError::Spawn(format!("couldn't start \"{}\": {e}", program.display())))?;
    let pid = child.id();

    #[cfg(windows)]
    let tree = {
        let job = match windows::Job::new() {
            Ok(job) => {
                if let Some(h) = child.raw_handle()
                    && let Err(e) = job.assign(h)
                {
                    tracing::warn!("couldn't add an MCP server to its job object: {e}");
                }
                Some(job)
            }
            Err(e) => {
                tracing::warn!("couldn't create a job object: {e}");
                None
            }
        };
        ProcessTree { job }
    };
    #[cfg(not(windows))]
    let tree = ProcessTree {};

    Ok(Spawned { child, pid, tree })
}

#[cfg(test)]
mod tests {
    #[cfg(any(windows, target_os = "linux"))]
    #[test]
    fn own_memory_is_reported() {
        let bytes = super::own_memory_bytes().expect("memory");
        assert!(bytes > 1024 * 1024, "{bytes}");
    }

    use super::*;

    fn exts() -> Vec<String> {
        vec![".com".into(), ".exe".into(), ".bat".into(), ".cmd".into()]
    }

    #[test]
    fn windows_names_resolve_through_pathext() {
        let dir = tempfile::tempdir().expect("tmp");
        // npm installs both `npx` (a shell script) and `npx.cmd`.
        std::fs::write(dir.path().join("npx"), "#!/bin/sh").expect("write");
        std::fs::write(dir.path().join("npx.cmd"), "@echo off").expect("write");
        std::fs::write(dir.path().join("uvx.exe"), "").expect("write");
        let path = dir.path().as_os_str();
        let found = resolve_with("npx", path, true, &exts()).expect("found");
        assert_eq!(found.file_name().and_then(|n| n.to_str()), Some("npx.cmd"));
        let found = resolve_with("uvx", path, true, &exts()).expect("found");
        assert_eq!(found.file_name().and_then(|n| n.to_str()), Some("uvx.exe"));
        let found = resolve_with("uvx.exe", path, true, &exts()).expect("found");
        assert_eq!(found.file_name().and_then(|n| n.to_str()), Some("uvx.exe"));
        let err = resolve_with("node", path, true, &exts()).expect_err("missing");
        assert!(err.to_string().contains("nodejs.org"), "{err}");
    }

    #[test]
    fn paths_with_separators_are_used_directly() {
        let dir = tempfile::tempdir().expect("tmp");
        let tool = dir.path().join("server.cmd");
        std::fs::write(&tool, "").expect("write");
        let without_ext = dir.path().join("server");
        let found = resolve_with(
            &without_ext.to_string_lossy(),
            OsStr::new(""),
            true,
            &exts(),
        )
        .expect("found");
        assert_eq!(found, tool);
        assert!(resolve_with("", OsStr::new(""), true, &exts()).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn unix_needs_the_executable_bit() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("tmp");
        let p = dir.path().join("srv");
        std::fs::write(&p, "#!/bin/sh").expect("write");
        assert!(resolve_with("srv", dir.path().as_os_str(), false, &[]).is_err());
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        assert_eq!(
            resolve_with("srv", dir.path().as_os_str(), false, &[]).expect("found"),
            p
        );
    }

    #[test]
    fn child_path_prefers_the_configured_path() {
        let mut env = BTreeMap::new();
        env.insert("Path".to_string(), "/custom/bin".to_string());
        let p = child_path(&env);
        let first = std::env::split_paths(&p).next().expect("one");
        assert_eq!(first, PathBuf::from("/custom/bin"));
    }
}
