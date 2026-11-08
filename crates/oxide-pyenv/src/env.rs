//! The managed Python environment: finding `uv`, installing CPython and the packages into a
//! virtual environment in the app's data folder, and reporting how that is going.
//!
//! Layout under `root` (`<app data>/python`):
//! - `pythons/`   CPython builds `uv python install` downloads
//! - `uv-cache/`  uv's package cache
//! - `venv/`      the virtual environment scripts run in
//! - `project/`   the embedded `pyproject.toml` + `uv.lock`, which `uv sync` reads
//! - `runner/`    the launcher scripts run through
//! - `work/<chat>/` a chat's working folder when it has no granted folder
//! - `runs/`      scripts being run (removed afterwards)
//! - `state.json` written when everything installed and the packages import

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use oxide_core::{
    PYTHON_PACKAGES, PYTHON_VERSION, PythonPackage, PythonState, PythonStatus, PythonStep,
};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};

use crate::error::{PythonError, Result};
use crate::{process, project, uv};

/// Longest one setup command (python install, sync) may take.
const STEP_LIMIT: Duration = Duration::from_secs(20 * 60);
/// Lines of `uv` output kept for the UI.
const LOG_LINES: usize = 60;
/// Least time between two progress events.
const EMIT_EVERY: Duration = Duration::from_millis(150);

/// Where the environment lives and how to find `uv`.
#[derive(Debug, Clone)]
pub struct PythonConfig {
    /// `<app data>/python`.
    pub root: PathBuf,
    /// Folders to look for `uv` in before the PATH.
    pub uv_dirs: Vec<PathBuf>,
    /// A `uv` to use instead (takes priority when it exists).
    pub uv: Option<PathBuf>,
    /// Packages the check step imports (and lists).
    pub packages: Vec<String>,
}

impl PythonConfig {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            uv_dirs: Vec::new(),
            uv: None,
            packages: PYTHON_PACKAGES.iter().map(|p| (*p).to_owned()).collect(),
        }
    }
}

/// Told about progress while the environment installs.
pub trait PythonSink: Send + Sync + 'static {
    fn changed(&self, status: &PythonStatus);
}

/// Ignores progress.
pub struct NoSink;

impl PythonSink for NoSink {
    fn changed(&self, _: &PythonStatus) {}
}

/// A usable environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ready {
    /// The environment's `python`.
    pub python: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Marker {
    schema: u32,
    project_hash: String,
    python_version: String,
    uv_version: String,
    packages: Vec<PythonPackage>,
}

#[derive(Default)]
struct Live {
    installing: bool,
    step: Option<PythonStep>,
    message: Option<String>,
    error: Option<String>,
    log: VecDeque<String>,
    uv_version: Option<String>,
    last_emit: Option<Instant>,
}

pub(crate) struct Inner {
    pub(crate) cfg: PythonConfig,
    sink: Arc<dyn PythonSink>,
    live: Mutex<Live>,
    /// One setup at a time; later callers wait for it and then find the environment ready.
    setup: tokio::sync::Mutex<()>,
    /// Scripts running now.
    pub(crate) running: AtomicUsize,
}

/// The managed Python environment. Cheap to clone.
#[derive(Clone)]
pub struct PythonEnv(pub(crate) Arc<Inner>);

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl PythonEnv {
    pub fn new(cfg: PythonConfig, sink: Arc<dyn PythonSink>) -> Self {
        Self(Arc::new(Inner {
            cfg,
            sink,
            live: Mutex::new(Live::default()),
            setup: tokio::sync::Mutex::new(()),
            running: AtomicUsize::new(0),
        }))
    }

    pub fn root(&self) -> &Path {
        &self.0.cfg.root
    }

    /// Scripts running now.
    pub fn running_scripts(&self) -> usize {
        self.0.running.load(std::sync::atomic::Ordering::SeqCst)
    }

    pub(crate) fn venv_dir(&self) -> PathBuf {
        self.root().join("venv")
    }

    /// The environment's interpreter.
    pub fn venv_python(&self) -> PathBuf {
        if cfg!(windows) {
            self.venv_dir().join("Scripts").join("python.exe")
        } else {
            self.venv_dir().join("bin").join("python")
        }
    }

    fn venv_bin(&self) -> PathBuf {
        if cfg!(windows) {
            self.venv_dir().join("Scripts")
        } else {
            self.venv_dir().join("bin")
        }
    }

    fn project_dir(&self) -> PathBuf {
        self.root().join("project")
    }

    pub(crate) fn runner_path(&self) -> PathBuf {
        self.root().join("runner").join("oxide_run.py")
    }

    fn marker_path(&self) -> PathBuf {
        self.root().join("state.json")
    }

    /// The `uv` this environment would use.
    pub fn find_uv(&self) -> Option<PathBuf> {
        uv::find(self.0.cfg.uv.as_deref(), &self.0.cfg.uv_dirs)
    }

    /// Environment variables for every `uv` call.
    fn uv_env(&self) -> Vec<(&'static str, Option<String>)> {
        let root = self.root();
        let path = |p: PathBuf| Some(p.to_string_lossy().into_owned());
        vec![
            ("UV_PYTHON_INSTALL_DIR", path(root.join("pythons"))),
            ("UV_CACHE_DIR", path(root.join("uv-cache"))),
            ("UV_PROJECT_ENVIRONMENT", path(self.venv_dir())),
            // Only Pythons uv installs itself: never a system Python.
            ("UV_PYTHON_PREFERENCE", Some("only-managed".into())),
            ("UV_PYTHON_DOWNLOADS", Some("automatic".into())),
            // Keep Oxide's Python to itself: no executables in the user's bin folder, no
            // Windows registry entries.
            ("UV_PYTHON_INSTALL_BIN", Some("0".into())),
            ("UV_PYTHON_INSTALL_REGISTRY", Some("0".into())),
            ("UV_NO_PROGRESS", Some("1".into())),
            ("UV_NO_CONFIG", Some("1".into())),
            ("NO_COLOR", Some("1".into())),
            ("VIRTUAL_ENV", None),
            ("PYTHONHOME", None),
            ("PYTHONPATH", None),
        ]
    }

    /// Environment variables for Python processes (setup checks and scripts).
    pub(crate) fn script_env(&self) -> Vec<(&'static str, Option<String>)> {
        let path = std::env::var_os("PATH").unwrap_or_default();
        let mut dirs = vec![self.venv_bin()];
        dirs.extend(std::env::split_paths(&path));
        let joined = std::env::join_paths(dirs)
            .map(|p| p.to_string_lossy().into_owned())
            .ok();
        vec![
            ("PYTHONUTF8", Some("1".into())),
            ("PYTHONIOENCODING", Some("utf-8".into())),
            ("PYTHONDONTWRITEBYTECODE", Some("1".into())),
            ("PYTHONUNBUFFERED", Some("1".into())),
            ("MPLBACKEND", Some("Agg".into())),
            (
                "MPLCONFIGDIR",
                Some(self.root().join("mpl").to_string_lossy().into_owned()),
            ),
            ("NO_COLOR", Some("1".into())),
            ("TERM", Some("dumb".into())),
            ("PATH", joined),
            (
                "VIRTUAL_ENV",
                Some(self.venv_dir().to_string_lossy().into_owned()),
            ),
            ("PYTHONHOME", None),
            ("PYTHONPATH", None),
            ("PYTHONSTARTUP", None),
        ]
    }

    // ---- Status ---------------------------------------------------------------------------

    async fn read_marker(&self) -> Option<Marker> {
        let text = tokio::fs::read_to_string(self.marker_path()).await.ok()?;
        serde_json::from_str(&text).ok()
    }

    /// The environment, if it is installed and up to date.
    pub async fn ready(&self) -> Option<Ready> {
        let marker = self.read_marker().await?;
        if marker.schema != 1 || marker.project_hash != project::project_hash() {
            return None;
        }
        let python = self.venv_python();
        tokio::fs::metadata(&python)
            .await
            .ok()
            .filter(|m| m.is_file())
            .map(|_| Ready { python })
    }

    fn live_status(&self) -> PythonStatus {
        let live = lock(&self.0.live);
        let mut s = PythonStatus::missing(self.root().to_string_lossy().into_owned());
        s.state = if live.installing {
            PythonState::Installing
        } else if live.error.is_some() {
            PythonState::Failed
        } else {
            PythonState::Missing
        };
        s.step = live.step;
        s.message = live.message.clone();
        s.error = live.error.clone();
        s.log = live.log.iter().cloned().collect();
        s.uv_version = live.uv_version.clone();
        s
    }

    /// The environment as Settings shows it. `with_disk` also measures the space it takes
    /// (walks the folders: don't call it for every progress event).
    pub async fn status(&self, with_disk: bool) -> PythonStatus {
        let mut s = self.live_status();
        let uv = self.find_uv();
        s.uv_path = uv.as_ref().map(|p| p.to_string_lossy().into_owned());
        if s.uv_version.is_none()
            && let Some(uv) = &uv
        {
            s.uv_version = self.uv_version(uv).await.ok();
        }
        if s.state != PythonState::Installing
            && let Some(_ready) = self.ready().await
            && let Some(m) = self.read_marker().await
        {
            s.state = PythonState::Ready;
            s.error = None;
            s.message = None;
            s.step = None;
            s.python_version = Some(m.python_version);
            s.packages = m.packages;
            if s.uv_version.is_none() {
                s.uv_version = Some(m.uv_version);
            }
        }
        if with_disk {
            let root = self.root().to_path_buf();
            s.disk_bytes = tokio::task::spawn_blocking(move || dir_size(&root))
                .await
                .ok()
                .filter(|n| *n > 0);
        }
        s
    }

    // ---- Progress ---------------------------------------------------------------------------

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
        let status = self.live_status();
        self.0.sink.changed(&status);
    }

    /// Sends the full status (read from disk, so it says "ready" once installed) when setup
    /// or reset has finished. Progress events use the live state, which only knows installing,
    /// failed and missing.
    async fn emit_settled(&self) {
        let status = self.status(true).await;
        lock(&self.0.live).last_emit = Some(Instant::now());
        self.0.sink.changed(&status);
    }

    fn push_log(&self, line: String) {
        let mut live = lock(&self.0.live);
        if live.log.len() >= LOG_LINES {
            live.log.pop_front();
        }
        live.log.push_back(line);
    }

    fn step(&self, step: PythonStep, message: &str) {
        {
            let mut live = lock(&self.0.live);
            live.step = Some(step);
            live.message = Some(message.to_owned());
        }
        self.push_log(format!("▸ {message}"));
        self.emit(true);
    }

    fn note(&self, line: impl Into<String>) {
        self.push_log(line.into());
        self.emit(false);
    }

    // ---- Install ----------------------------------------------------------------------------

    /// Makes sure the environment is installed, installing it when it isn't (first use,
    /// after a repair or reset, or after an update changed the packages). Concurrent calls
    /// share one installation.
    pub async fn ensure(&self) -> Result<Ready> {
        if let Some(r) = self.ready().await {
            return Ok(r);
        }
        let _one_at_a_time = self.0.setup.lock().await;
        if let Some(r) = self.ready().await {
            return Ok(r);
        }
        {
            let mut live = lock(&self.0.live);
            live.installing = true;
            live.error = None;
            live.step = None;
            live.message = Some("Starting".into());
            live.log.clear();
        }
        self.emit(true);
        let result = self.install().await;
        {
            let mut live = lock(&self.0.live);
            live.installing = false;
            match &result {
                Ok(_) => {
                    live.error = None;
                    live.step = None;
                    live.message = None;
                }
                Err(e) => live.error = Some(e.to_string()),
            }
        }
        if let Err(e) = &result {
            tracing::warn!("Python setup failed: {e}");
        }
        self.emit_settled().await;
        result
    }

    async fn install(&self) -> Result<Ready> {
        let root = self.root().to_path_buf();
        self.step(PythonStep::Uv, "Looking for uv");
        let uv = self.find_uv().ok_or(PythonError::NoUv)?;
        let version = self.uv_version(&uv).await?;
        lock(&self.0.live).uv_version = Some(version.clone());
        self.note(format!("{version} · {}", uv.display()));

        let project_dir = self.project_dir();
        for dir in [&project_dir, &root.join("runner"), &root.join("mpl")] {
            tokio::fs::create_dir_all(dir)
                .await
                .map_err(|e| PythonError::io(format!("Couldn't create {}", dir.display()), e))?;
        }
        write(&project_dir.join("pyproject.toml"), project::PYPROJECT).await?;
        let lock_path = project_dir.join("uv.lock");
        if project::LOCK.trim().is_empty() {
            let _ = tokio::fs::remove_file(&lock_path).await;
        } else {
            write(&lock_path, project::LOCK).await?;
        }
        write(&self.runner_path(), project::RUNNER).await?;

        self.step(
            PythonStep::Python,
            &format!("Installing Python {PYTHON_VERSION} (a one-time download)"),
        );
        self.uv(
            &uv,
            &["python", "install", PYTHON_VERSION],
            "Installing Python",
        )
        .await?;

        let python = self.venv_python();
        if tokio::fs::metadata(&python).await.is_ok() {
            self.step(PythonStep::Venv, "Reusing the environment");
        } else {
            self.step(PythonStep::Venv, "Creating the environment");
            remove_dir(&self.venv_dir()).await?;
            let venv = self.venv_dir().to_string_lossy().into_owned();
            self.uv(
                &uv,
                &["venv", &venv, "--python", PYTHON_VERSION],
                "Creating the environment",
            )
            .await?;
        }

        self.step(PythonStep::Packages, "Installing packages");
        let project_arg = project_dir.to_string_lossy().into_owned();
        let mut sync = vec![
            "sync",
            "--project",
            &project_arg,
            "--python",
            PYTHON_VERSION,
            "--no-install-project",
        ];
        if !project::LOCK.trim().is_empty() {
            sync.push("--locked");
        }
        self.uv(&uv, &sync, "Installing packages").await?;

        self.step(PythonStep::Check, "Checking the packages");
        let (python_version, packages) = self.check().await?;
        let marker = Marker {
            schema: 1,
            project_hash: project::project_hash(),
            python_version,
            uv_version: version,
            packages,
        };
        let text =
            serde_json::to_string_pretty(&marker).map_err(|e| PythonError::Spawn(e.to_string()))?;
        write(&self.marker_path(), &text).await?;
        self.note("Ready");
        Ok(Ready { python })
    }

    /// Imports the packages once and reads their versions.
    async fn check(&self) -> Result<(String, Vec<PythonPackage>)> {
        let names = serde_json::to_string(&self.0.cfg.packages)
            .map_err(|e| PythonError::Spawn(e.to_string()))?;
        let script = format!(
            "import importlib, json, sys\n\
             from importlib import metadata\n\
             names = {names}\n\
             out = {{'python': sys.version.split()[0], 'packages': []}}\n\
             for name in names:\n\
             \x20   importlib.import_module(name)\n\
             \x20   out['packages'].append({{'name': name, 'version': metadata.version(name)}})\n\
             print(json.dumps(out))\n"
        );
        let mut cmd = tokio::process::Command::new(self.venv_python());
        cmd.args(["-I", "-c", &script])
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
        let mut child = cmd
            .spawn()
            .map_err(|e| PythonError::Spawn(format!("{}: {e}", self.venv_python().display())))?;
        let pid = child.id();
        let mut out = child.stdout.take();
        let mut err = child.stderr.take();
        let waited = tokio::time::timeout(Duration::from_secs(300), async {
            tokio::join!(
                process::capture(out.as_mut(), 1 << 20),
                process::capture(err.as_mut(), 1 << 20),
                child.wait()
            )
        })
        .await;
        let Ok((stdout, stderr, status)) = waited else {
            process::kill_tree(pid).await;
            return Err(PythonError::Setup {
                step: "Checking the packages".into(),
                detail: "Python didn't answer within 5 minutes.".into(),
            });
        };
        let ok = status.is_ok_and(|s| s.success());
        if !ok {
            let detail = tail_text(&String::from_utf8_lossy(&stderr), 12);
            self.push_log(detail.clone());
            return Err(PythonError::Setup {
                step: "Checking the packages".into(),
                detail,
            });
        }
        #[derive(Deserialize)]
        struct Out {
            python: String,
            packages: Vec<PythonPackage>,
        }
        let parsed: Out = serde_json::from_slice(&stdout).map_err(|e| PythonError::Setup {
            step: "Checking the packages".into(),
            detail: format!("Unexpected answer from Python: {e}"),
        })?;
        Ok((parsed.python, parsed.packages))
    }

    async fn uv_version(&self, uv: &Path) -> Result<String> {
        let mut cmd = tokio::process::Command::new(uv);
        cmd.arg("--version")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        process::prepare(&mut cmd);
        let out = tokio::time::timeout(Duration::from_secs(30), cmd.output())
            .await
            .map_err(|_| PythonError::Setup {
                step: "Finding uv".into(),
                detail: format!("{} didn't answer `--version`.", uv.display()),
            })?
            .map_err(|e| PythonError::Spawn(format!("{}: {e}", uv.display())))?;
        if !out.status.success() {
            return Err(PythonError::Setup {
                step: "Finding uv".into(),
                detail: format!(
                    "{} doesn't run (exit code {:?}).",
                    uv.display(),
                    out.status.code()
                ),
            });
        }
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
    }

    /// Runs `uv` with `args`, passing its output to the progress log.
    async fn uv(&self, uv: &Path, args: &[&str], what: &str) -> Result<()> {
        let mut cmd = tokio::process::Command::new(uv);
        cmd.args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (k, v) in self.uv_env() {
            match v {
                Some(v) => cmd.env(k, v),
                None => cmd.env_remove(k),
            };
        }
        process::prepare(&mut cmd);
        let mut child = cmd
            .spawn()
            .map_err(|e| PythonError::Spawn(format!("{}: {e}", uv.display())))?;
        let pid = child.id();
        let out = child.stdout.take();
        let err = child.stderr.take();
        let tail = Mutex::new(VecDeque::<String>::new());
        let waited = tokio::time::timeout(STEP_LIMIT, async {
            let (_, _, status) =
                tokio::join!(self.pump(out, &tail), self.pump(err, &tail), child.wait());
            status
        })
        .await;
        let status = match waited {
            Ok(s) => s,
            Err(_) => {
                process::kill_tree(pid).await;
                return Err(PythonError::Setup {
                    step: what.to_owned(),
                    detail: format!("It took longer than {} minutes.", STEP_LIMIT.as_secs() / 60),
                });
            }
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
                Err(PythonError::Setup {
                    step: what.to_owned(),
                    detail: format!("uv ended with {code}.\n{}", lines.join("\n")),
                })
            }
        }
    }

    async fn pump<R: AsyncRead + Unpin>(&self, r: Option<R>, tail: &Mutex<VecDeque<String>>) {
        let Some(r) = r else { return };
        let mut lines = BufReader::new(r).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let line = line.trim_end().to_owned();
            if line.is_empty() {
                continue;
            }
            {
                let mut t = lock(tail);
                if t.len() >= 12 {
                    t.pop_front();
                }
                t.push_back(line.clone());
            }
            self.note(line);
        }
    }

    // ---- Repair and reset ------------------------------------------------------------------

    fn refuse_while_running(&self) -> Result<()> {
        if self.0.running.load(std::sync::atomic::Ordering::SeqCst) > 0 {
            return Err(PythonError::Busy);
        }
        Ok(())
    }

    /// Throws the virtual environment away and installs it again. Downloaded Pythons and the
    /// package cache stay, so this is quick.
    pub async fn repair(&self) -> Result<Ready> {
        self.refuse_while_running()?;
        let _one_at_a_time = self.0.setup.lock().await;
        remove_dir(&self.venv_dir()).await?;
        let _ = tokio::fs::remove_file(self.marker_path()).await;
        drop(_one_at_a_time);
        self.ensure().await
    }

    /// Removes the environment, the downloaded Pythons and the cache. Chats' working folders
    /// stay. The next `run_python` (or "Set up now") installs everything again.
    pub async fn reset(&self) -> Result<()> {
        self.refuse_while_running()?;
        let _one_at_a_time = self.0.setup.lock().await;
        let _ = tokio::fs::remove_file(self.marker_path()).await;
        for dir in [
            "venv", "pythons", "uv-cache", "project", "runner", "runs", "mpl",
        ] {
            remove_dir(&self.root().join(dir)).await?;
        }
        {
            let mut live = lock(&self.0.live);
            *live = Live::default();
        }
        self.emit_settled().await;
        Ok(())
    }
}

async fn write(path: &Path, text: &str) -> Result<()> {
    tokio::fs::write(path, text)
        .await
        .map_err(|e| PythonError::io(format!("Couldn't write {}", path.display()), e))
}

/// Removes a folder (not an error when it isn't there), retrying a little for Windows file
/// locks.
pub(crate) async fn remove_dir(path: &Path) -> Result<()> {
    for attempt in 0..4 {
        match tokio::fs::remove_dir_all(path).await {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) if attempt == 3 => {
                return Err(PythonError::io(
                    format!("Couldn't remove {}", path.display()),
                    e,
                ));
            }
            Err(_) => tokio::time::sleep(Duration::from_millis(250)).await,
        }
    }
    Ok(())
}

fn tail_text(text: &str, lines: usize) -> String {
    let all: Vec<&str> = text.lines().collect();
    let from = all.len().saturating_sub(lines);
    all[from..].join("\n")
}

fn dir_size(path: &Path) -> u64 {
    let mut total = 0;
    let Ok(rd) = std::fs::read_dir(path) else {
        return 0;
    };
    for e in rd.flatten() {
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_dir() {
            total += dir_size(&e.path());
        } else if ft.is_file() {
            total += e.metadata().map(|m| m.len()).unwrap_or(0);
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tail_keeps_the_last_lines() {
        assert_eq!(tail_text("a\nb\nc\nd", 2), "c\nd");
        assert_eq!(tail_text("", 3), "");
    }

    #[test]
    fn script_env_puts_the_venv_first_and_pins_matplotlib() {
        let env = PythonEnv::new(PythonConfig::new("/tmp/x/python"), Arc::new(NoSink));
        let vars = env.script_env();
        let get = |k: &str| {
            vars.iter()
                .find(|(n, _)| *n == k)
                .and_then(|(_, v)| v.clone())
        };
        assert_eq!(get("MPLBACKEND").as_deref(), Some("Agg"));
        assert!(
            get("PATH")
                .expect("PATH")
                .starts_with(&env.venv_bin().to_string_lossy().into_owned())
        );
        assert!(vars.iter().any(|(k, v)| *k == "PYTHONPATH" && v.is_none()));
    }
}
