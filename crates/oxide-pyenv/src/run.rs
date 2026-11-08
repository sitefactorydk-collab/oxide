//! Running a script in the managed environment.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crate::env::{Inner, PythonEnv, remove_dir};
use crate::error::{PythonError, Result};
use crate::{process, project};

/// Most bytes read from each of a script's output streams (the rest is drained and dropped).
const MAX_CAPTURE_BYTES: usize = 4 * 1024 * 1024;

static RUN_COUNTER: AtomicU64 = AtomicU64::new(0);

/// A script to run.
#[derive(Debug, Clone)]
pub struct RunRequest {
    pub code: String,
    /// The folder it runs in (it is created when missing).
    pub cwd: PathBuf,
    /// Longer than this and the script (and what it started) is stopped.
    pub timeout: Duration,
}

/// What a script did.
#[derive(Debug, Clone, PartialEq)]
pub struct RunOutput {
    /// `None` when it was ended by a signal or the time limit.
    pub exit_code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub timed_out: bool,
    pub secs: f32,
}

struct Running(Arc<Inner>);

impl Drop for Running {
    fn drop(&mut self) {
        self.0.running.fetch_sub(1, Ordering::SeqCst);
    }
}

impl PythonEnv {
    /// A chat's working folder (when it has no granted folder). Not created yet.
    pub fn work_dir(&self, chat_id: &str) -> PathBuf {
        let safe: String = chat_id
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .take(64)
            .collect();
        self.root().join("work").join(safe)
    }

    /// Deletes a chat's working folder (the chat was deleted).
    pub async fn forget_chat(&self, chat_id: &str) {
        if let Err(e) = remove_dir(&self.work_dir(chat_id)).await {
            tracing::warn!("couldn't remove a chat's Python folder: {e}");
        }
    }

    /// Runs `req.code` in the environment, installing it first when needed (progress goes to
    /// the sink; the time limit starts afterwards).
    pub async fn run(&self, req: RunRequest) -> Result<RunOutput> {
        let ready = self.ensure().await?;
        self.0.running.fetch_add(1, Ordering::SeqCst);
        let _running = Running(self.0.clone());

        tokio::fs::create_dir_all(&req.cwd)
            .await
            .map_err(|e| PythonError::io(format!("Couldn't create {}", req.cwd.display()), e))?;
        self.write_runner().await?;
        let id = format!(
            "{}-{}",
            std::process::id(),
            RUN_COUNTER.fetch_add(1, Ordering::SeqCst)
        );
        let dir = self.root().join("runs").join(&id);
        tokio::fs::create_dir_all(&dir)
            .await
            .map_err(|e| PythonError::io("Couldn't create the script folder", e))?;
        let script = dir.join("main.py");
        tokio::fs::write(&script, &req.code)
            .await
            .map_err(|e| PythonError::io("Couldn't write the script", e))?;

        let out = self.spawn_and_wait(&ready.python, &script, &req).await;
        let _ = remove_dir(&dir).await;
        out
    }

    async fn write_runner(&self) -> Result<()> {
        let path = self.runner_path();
        if tokio::fs::read_to_string(&path).await.ok().as_deref() == Some(project::RUNNER) {
            return Ok(());
        }
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| PythonError::io("Couldn't create the runner folder", e))?;
        }
        tokio::fs::write(&path, project::RUNNER)
            .await
            .map_err(|e| PythonError::io("Couldn't write the runner", e))
    }

    async fn spawn_and_wait(
        &self,
        python: &Path,
        script: &Path,
        req: &RunRequest,
    ) -> Result<RunOutput> {
        let mut cmd = tokio::process::Command::new(python);
        cmd.args(["-X", "utf8", "-u", "-B"])
            .arg(self.runner_path())
            .arg(script)
            .current_dir(&req.cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (k, v) in self.script_env() {
            match v {
                Some(v) => cmd.env(k, v),
                None => cmd.env_remove(k),
            };
        }
        process::prepare(&mut cmd);
        let started = Instant::now();
        let mut child = cmd
            .spawn()
            .map_err(|e| PythonError::Spawn(format!("{}: {e}", python.display())))?;
        let pid = child.id();
        let mut out = child.stdout.take();
        let mut err = child.stderr.take();
        let waited = tokio::time::timeout(req.timeout, async {
            tokio::join!(
                process::capture(out.as_mut(), MAX_CAPTURE_BYTES),
                process::capture(err.as_mut(), MAX_CAPTURE_BYTES),
                child.wait()
            )
        })
        .await;
        match waited {
            Ok((stdout, stderr, status)) => {
                let status = status.map_err(|e| PythonError::Spawn(e.to_string()))?;
                Ok(RunOutput {
                    exit_code: status.code(),
                    stdout,
                    stderr,
                    timed_out: false,
                    secs: started.elapsed().as_secs_f32(),
                })
            }
            Err(_) => {
                process::kill_tree(pid).await;
                Ok(RunOutput {
                    exit_code: None,
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                    timed_out: true,
                    secs: req.timeout.as_secs_f32(),
                })
            }
        }
    }
}
