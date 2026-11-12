//! Running the `docker` program: finding it, bounded output capture and time limits.
//!
//! Oxide talks to Docker through its command line rather than the engine's API, so whatever
//! the user's `docker` is set up for (Docker Desktop's context, a remote engine, Podman's
//! `docker` shim) works the same way.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use tokio::io::AsyncReadExt;

use crate::SandboxError;

/// Most bytes kept of each output stream (the rest is read and dropped).
pub const MAX_CAPTURE_BYTES: usize = 4 * 1024 * 1024;

/// The `docker` program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Docker {
    pub program: PathBuf,
}

/// What one `docker` call printed.
#[derive(Debug, Clone, Default)]
pub struct Output {
    /// `None` when it was stopped (time limit) or ended by a signal.
    pub code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub timed_out: bool,
    pub elapsed: Duration,
}

impl Output {
    pub fn ok(&self) -> bool {
        self.code == Some(0)
    }

    pub fn stdout_text(&self) -> String {
        String::from_utf8_lossy(&self.stdout).trim().to_owned()
    }

    /// Docker's error message (stderr, else stdout), one line per line, without the
    /// "Error response from daemon:" prefix.
    pub fn error_text(&self) -> String {
        let err = String::from_utf8_lossy(&self.stderr);
        let text = if err.trim().is_empty() {
            String::from_utf8_lossy(&self.stdout).into_owned()
        } else {
            err.into_owned()
        };
        let text = text
            .lines()
            .map(|l| {
                l.trim()
                    .trim_start_matches("Error response from daemon:")
                    .trim()
            })
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        if text.is_empty() {
            match self.code {
                Some(c) => format!("docker exited with code {c}"),
                None if self.timed_out => "docker took too long".into(),
                None => "docker was stopped".into(),
            }
        } else {
            text
        }
    }

    /// `Ok(stdout)` when the call succeeded, else Docker's error.
    pub fn into_result(self) -> Result<String, SandboxError> {
        if self.ok() {
            Ok(self.stdout_text())
        } else {
            Err(SandboxError::Docker(self.error_text()))
        }
    }
}

/// Folders where Docker installs its command line, searched after `PATH` (apps started from
/// the desktop often get a shorter `PATH` than a terminal).
fn extra_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if cfg!(windows) {
        for var in ["ProgramFiles", "ProgramW6432"] {
            if let Some(p) = std::env::var_os(var) {
                dirs.push(
                    PathBuf::from(p)
                        .join("Docker")
                        .join("Docker")
                        .join("resources")
                        .join("bin"),
                );
            }
        }
        if let Some(p) = std::env::var_os("LOCALAPPDATA") {
            dirs.push(PathBuf::from(p).join("Programs").join("Docker"));
        }
    } else {
        dirs.push("/usr/local/bin".into());
        dirs.push("/opt/homebrew/bin".into());
        dirs.push("/usr/bin".into());
        dirs.push("/Applications/Docker.app/Contents/Resources/bin".into());
    }
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" });
    if let Some(h) = home.filter(|h| !h.is_empty()) {
        dirs.push(PathBuf::from(h).join(".docker").join("bin"));
    }
    dirs
}

impl Docker {
    /// Finds `docker`: `explicit` when given (and a file), else on `PATH`, else in the usual
    /// install folders.
    pub fn find(explicit: Option<&Path>) -> Option<Self> {
        if let Some(p) = explicit {
            return p.is_file().then(|| Self {
                program: p.to_path_buf(),
            });
        }
        let exe = if cfg!(windows) {
            "docker.exe"
        } else {
            "docker"
        };
        let path = std::env::var_os("PATH").unwrap_or_default();
        std::env::split_paths(&path)
            .chain(extra_dirs())
            .map(|d| d.join(exe))
            .find(|p| p.is_file())
            .map(|program| Self { program })
    }

    fn command<I, S>(&self, args: I) -> tokio::process::Command
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut cmd = tokio::process::Command::new(&self.program);
        cmd.args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(windows)]
        {
            // CREATE_NO_WINDOW: no console window flashes up.
            cmd.creation_flags(0x0800_0000);
        }
        cmd
    }

    /// Runs `docker <args>` and waits up to `timeout`. A call past its limit is killed (the
    /// `docker` client only: callers stop what it started inside a container themselves).
    pub async fn run<I, S>(&self, args: I, timeout: Duration) -> Result<Output, SandboxError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let started = Instant::now();
        let mut child = self.command(args).spawn().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                SandboxError::DockerMissing
            } else {
                SandboxError::Docker(format!("docker couldn't start: {e}"))
            }
        })?;
        let mut out = child.stdout.take();
        let mut err = child.stderr.take();
        let waited = tokio::time::timeout(timeout, async {
            tokio::join!(capture(out.as_mut()), capture(err.as_mut()), child.wait())
        })
        .await;
        match waited {
            Ok((stdout, stderr, status)) => {
                let status = status?;
                Ok(Output {
                    code: status.code(),
                    stdout,
                    stderr,
                    timed_out: false,
                    elapsed: started.elapsed(),
                })
            }
            Err(_) => {
                let _ = child.kill().await;
                Ok(Output {
                    code: None,
                    timed_out: true,
                    elapsed: started.elapsed(),
                    ..Output::default()
                })
            }
        }
    }

    /// Like [`Docker::run`], failing unless docker exits with 0. Returns stdout, trimmed.
    pub async fn ok<I, S>(&self, args: I, timeout: Duration) -> Result<String, SandboxError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.run(args, timeout).await?.into_result()
    }
}

/// Reads a stream to its end, keeping at most [`MAX_CAPTURE_BYTES`].
async fn capture<R: tokio::io::AsyncRead + Unpin>(r: Option<&mut R>) -> Vec<u8> {
    let Some(r) = r else { return Vec::new() };
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        match r.read(&mut chunk).await {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                if buf.len() < MAX_CAPTURE_BYTES {
                    let take = n.min(MAX_CAPTURE_BYTES - buf.len());
                    buf.extend_from_slice(&chunk[..take]);
                }
            }
        }
    }
    buf
}

/// What Docker's "engine isn't running" errors look like, turned into one readable sentence.
pub fn stopped_message(raw: &str) -> String {
    let lower = raw.to_ascii_lowercase();
    if lower.contains("cannot connect")
        || lower.contains("is the docker daemon running")
        || lower.contains("pipe")
        || lower.contains("connection refused")
        || lower.contains("docker desktop")
        || lower.is_empty()
    {
        "Docker is installed, but its engine isn't answering. Start Docker Desktop (or the \
         Docker service) and try again."
            .into()
    } else {
        raw.lines().next().unwrap_or(raw).trim().to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_text_drops_the_daemon_prefix() {
        let o = Output {
            code: Some(1),
            stderr: b"Error response from daemon: No such container: x\n".to_vec(),
            ..Output::default()
        };
        assert_eq!(o.error_text(), "No such container: x");
        let o = Output {
            code: Some(3),
            ..Output::default()
        };
        assert_eq!(o.error_text(), "docker exited with code 3");
    }

    #[test]
    fn stopped_messages_are_readable() {
        let m = stopped_message(
            "Cannot connect to the Docker daemon at unix:///var/run/docker.sock. Is the docker \
             daemon running?",
        );
        assert!(m.contains("Start Docker Desktop"), "{m}");
        let m = stopped_message(
            "error during connect: open //./pipe/dockerDesktopLinuxEngine: The system cannot \
             find the file specified.",
        );
        assert!(m.contains("Start Docker Desktop"), "{m}");
    }

    #[test]
    fn find_takes_an_explicit_file_only() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert_eq!(Docker::find(Some(&dir.path().join("nope"))), None);
        let p = dir.path().join("docker");
        std::fs::write(&p, "").expect("write");
        assert_eq!(Docker::find(Some(&p)).map(|d| d.program), Some(p));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn run_captures_and_times_out() {
        let d = Docker {
            program: "sh".into(),
        };
        let o = d
            .run(
                ["-c", "echo out; echo err >&2; exit 3"],
                Duration::from_secs(10),
            )
            .await
            .expect("runs");
        assert_eq!(o.code, Some(3));
        assert_eq!(o.stdout_text(), "out");
        assert_eq!(o.error_text(), "err");
        let o = d
            .run(["-c", "sleep 5"], Duration::from_millis(200))
            .await
            .expect("runs");
        assert!(o.timed_out);
        assert!(o.elapsed < Duration::from_secs(4));
        let missing = Docker {
            program: "/no/such/docker".into(),
        };
        assert!(matches!(
            missing.run(["version"], Duration::from_secs(1)).await,
            Err(SandboxError::DockerMissing)
        ));
    }
}
