//! Per-chat workspace containers: creating, starting, stopping when idle, commands, file
//! listings, copies in and out, and reset.
//!
//! Every container Oxide makes carries the label `oxide.sandbox=1` plus `oxide.chat`,
//! `oxide.config` (what it was made with, see `SandboxSettings::container_key`) and
//! `oxide.workspace` (`volume:<name>` or `folder:<path>`), so Oxide can find its containers
//! again after a restart and nothing else is touched.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use oxide_core::sandbox::{
    ChatSandbox, DOCKER_INSTALL_URL, DockerState, DockerStatus, MAX_LISTING, SandboxCommand,
    SandboxFile, SandboxImage, SandboxListing, SandboxSettings, SandboxState, SandboxStorage,
    WORKSPACE_DIR, base_name, sandbox_name, workspace_path,
};
use oxide_mcp::McpClient;
use tokio::task::JoinHandle;

use crate::SandboxError;
use crate::docker::{Docker, stopped_message};
use crate::script;

/// Ordinary Docker calls (inspect, start, stop, ls).
const QUICK: Duration = Duration::from_secs(30);
/// Checking whether the engine answers.
const PROBE: Duration = Duration::from_secs(15);
/// How long a status stays fresh.
const STATUS_TTL: Duration = Duration::from_secs(20);
/// Creating a container (the image is already there).
const CREATE: Duration = Duration::from_secs(120);
/// Downloading an image.
const PULL: Duration = Duration::from_secs(60 * 60);
/// Copying in or out.
const COPY: Duration = Duration::from_secs(30 * 60);
/// Largest copy in or out of a sandbox.
pub const MAX_COPY_BYTES: u64 = 2 * 1024 * 1024 * 1024;
/// How often idle sandboxes are looked for.
const REAP_EVERY: Duration = Duration::from_secs(60);

/// Docker's label on everything Oxide makes.
const LABEL: &str = "oxide.sandbox=1";

/// Capabilities a sandbox keeps: enough for package managers (which change file owners and
/// drop to their own users) and nothing that reaches past the container.
const CAPS: [&str; 7] = [
    "CHOWN",
    "DAC_OVERRIDE",
    "FOWNER",
    "FSETID",
    "SETUID",
    "SETGID",
    "KILL",
];

/// Told when Docker's status or a chat's sandbox changed.
pub trait SandboxSink: Send + Sync + 'static {
    /// `None`: Docker or the images; `Some(id)`: that chat's sandbox.
    fn changed(&self, conversation_id: Option<&str>);
}

/// Ignores changes (tests).
pub struct NoSink;

impl SandboxSink for NoSink {
    fn changed(&self, _conversation_id: Option<&str>) {}
}

/// What a command printed and how it ended.
#[derive(Debug, Clone)]
pub struct ExecResult {
    /// The folder it ran in.
    pub cwd: String,
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
    pub elapsed: Duration,
}

#[derive(Default)]
struct ChatState {
    last_used: Option<Instant>,
    commands: Vec<SandboxCommand>,
    files: Option<Arc<McpClient>>,
    starting: bool,
    error: Option<String>,
}

/// A container as Docker describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Existing {
    running: bool,
    image: String,
    key: String,
    workspace: String,
}

/// Where a chat's files live, from the `oxide.workspace` label.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Workspace {
    Volume(String),
    Folder(PathBuf),
}

impl Workspace {
    fn parse(label: &str) -> Option<Self> {
        if let Some(v) = label.strip_prefix("volume:") {
            return (!v.is_empty()).then(|| Self::Volume(v.to_owned()));
        }
        label
            .strip_prefix("folder:")
            .filter(|p| !p.is_empty())
            .map(|p| Self::Folder(PathBuf::from(p)))
    }

    fn label(&self) -> String {
        match self {
            Self::Volume(v) => format!("volume:{v}"),
            Self::Folder(p) => format!("folder:{}", plain(p)),
        }
    }

    /// The `--mount` value that puts it at `/workspace`.
    fn mount(&self) -> Result<String, SandboxError> {
        match self {
            Self::Volume(v) => Ok(format!("type=volume,src={v},dst={WORKSPACE_DIR}")),
            Self::Folder(p) => {
                let p = plain(p);
                if p.contains(',') || p.contains('"') {
                    return Err(SandboxError::Invalid(format!(
                        "The sandbox folder “{p}” has a comma or quote in its path, which \
                         Docker can't mount. Pick another folder in Settings → Sandbox."
                    )));
                }
                Ok(format!("type=bind,src={p},dst={WORKSPACE_DIR}"))
            }
        }
    }

    fn display(&self) -> String {
        match self {
            Self::Volume(v) => v.clone(),
            Self::Folder(p) => plain(p),
        }
    }
}

/// A path as Docker and people want it on Windows: without the `\\?\` prefix.
pub(crate) fn plain(p: &Path) -> String {
    let s = p.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        rest.to_owned()
    } else {
        s.into_owned()
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// The cached file-server tools.
pub(crate) fn lock_tools(
    m: &Mutex<Option<Vec<oxide_core::McpTool>>>,
) -> std::sync::MutexGuard<'_, Option<Vec<oxide_core::McpTool>>> {
    lock(m)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

pub(crate) struct Inner {
    /// `<app data>/sandboxes`: default home of folder workspaces.
    pub(crate) root: PathBuf,
    /// Where the file server's log goes (`sandbox-files.log`).
    pub(crate) logs: Option<PathBuf>,
    pub(crate) sink: Arc<dyn SandboxSink>,
    /// A `docker` to use instead of searching (tests).
    explicit: Option<PathBuf>,
    settings: Mutex<SandboxSettings>,
    status: Mutex<(DockerStatus, Option<Instant>)>,
    chats: Mutex<HashMap<String, ChatState>>,
    /// One `ensure` (and one file server start) at a time per chat.
    locks: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    pulls: tokio::sync::Mutex<()>,
    pub(crate) file_tools: Mutex<Option<Vec<oxide_core::McpTool>>>,
    next: AtomicU64,
    reaper: Mutex<Option<JoinHandle<()>>>,
}

/// The sandboxes of all chats. Cheap to clone.
#[derive(Clone)]
pub struct Sandboxes {
    pub(crate) inner: Arc<Inner>,
}

impl Sandboxes {
    /// `root`: Oxide's folder for sandbox data (`<app data>/sandboxes`); `logs`: the log
    /// folder. Docker is looked for when first needed.
    pub fn new(root: PathBuf, logs: Option<PathBuf>, sink: Arc<dyn SandboxSink>) -> Self {
        Self::with_docker(root, logs, sink, None)
    }

    /// Like [`Sandboxes::new`] with a given `docker` program (tests).
    pub fn with_docker(
        root: PathBuf,
        logs: Option<PathBuf>,
        sink: Arc<dyn SandboxSink>,
        docker: Option<PathBuf>,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                root,
                logs,
                sink,
                explicit: docker,
                settings: Mutex::new(SandboxSettings::default()),
                status: Mutex::new((
                    DockerStatus {
                        install_url: DOCKER_INSTALL_URL.into(),
                        ..DockerStatus::default()
                    },
                    None,
                )),
                chats: Mutex::new(HashMap::new()),
                locks: Mutex::new(HashMap::new()),
                pulls: tokio::sync::Mutex::new(()),
                file_tools: Mutex::new(None),
                next: AtomicU64::new(1),
                reaper: Mutex::new(None),
            }),
        }
    }

    /// Takes new settings (called when settings are saved and before each reply).
    pub fn configure(&self, settings: &SandboxSettings) {
        let next = settings.clone().normalized();
        let images_changed = {
            let mut cur = lock(&self.inner.settings);
            let changed = cur.image != next.image || cur.files_image != next.files_image;
            *cur = next;
            changed
        };
        if images_changed {
            // The image list in the status is for the old names.
            lock(&self.inner.status).1 = None;
        }
    }

    pub fn settings(&self) -> SandboxSettings {
        lock(&self.inner.settings).clone()
    }

    pub(crate) fn changed(&self, id: Option<&str>) {
        self.inner.sink.changed(id);
    }

    fn find_docker(&self) -> Option<Docker> {
        Docker::find(self.inner.explicit.as_deref())
    }

    /// The `docker` program, whether or not its engine runs.
    pub(crate) fn docker(&self) -> Result<Docker, SandboxError> {
        self.find_docker().ok_or(SandboxError::DockerMissing)
    }

    // ---- Docker's status ------------------------------------------------------------------

    /// Whether Docker can be used, and the images. Cached for a few seconds unless
    /// `refresh`.
    pub async fn docker_status(&self, refresh: bool) -> DockerStatus {
        {
            let (status, at) = &*lock(&self.inner.status);
            if !refresh && at.is_some_and(|t| t.elapsed() < STATUS_TTL) {
                return status.clone();
            }
        }
        let fresh = self.probe().await;
        let changed = {
            let mut guard = lock(&self.inner.status);
            let pulling = guard.0.pulling.clone();
            let mut fresh = fresh;
            fresh.pulling = pulling;
            let changed = {
                let old = &guard.0;
                old.state != fresh.state
                    || old.images != fresh.images
                    || old.message != fresh.message
            };
            *guard = (fresh, Some(Instant::now()));
            changed
        };
        if changed {
            self.changed(None);
        }
        lock(&self.inner.status).0.clone()
    }

    async fn probe(&self) -> DockerStatus {
        let mut st = DockerStatus {
            install_url: DOCKER_INSTALL_URL.into(),
            checked_at: now_ms(),
            ..DockerStatus::default()
        };
        let Some(d) = self.find_docker() else {
            st.state = DockerState::Missing;
            st.message = Some(SandboxError::DockerMissing.to_string());
            return st;
        };
        st.program = Some(plain(&d.program));
        match d
            .run(["version", "--format", "{{.Server.Version}}"], PROBE)
            .await
        {
            Ok(o) if o.ok() && !o.stdout_text().is_empty() => {
                st.state = DockerState::Ready;
                st.version = Some(o.stdout_text());
            }
            Ok(o) if o.timed_out => {
                st.state = DockerState::Stopped;
                st.message = Some(
                    "Docker didn't answer in time. If Docker Desktop is starting, wait for it \
                     and try again."
                        .into(),
                );
                return st;
            }
            Ok(o) => {
                st.state = DockerState::Stopped;
                st.message = Some(stopped_message(&o.error_text()));
                return st;
            }
            Err(SandboxError::DockerMissing) => {
                st.state = DockerState::Missing;
                st.message = Some(SandboxError::DockerMissing.to_string());
                return st;
            }
            Err(e) => {
                st.state = DockerState::Stopped;
                st.message = Some(e.to_string());
                return st;
            }
        }
        if let Ok(o) = d
            .run(["info", "--format", "{{.NCPU}}|{{.MemTotal}}"], PROBE)
            .await
            && o.ok()
        {
            let text = o.stdout_text();
            let mut parts = text.split('|');
            st.cpus = parts.next().and_then(|c| c.trim().parse().ok());
            st.memory_bytes = parts.next().and_then(|m| m.trim().parse().ok());
        }
        let settings = self.settings();
        for (name, role) in [
            (&settings.image, "Commands"),
            (&settings.files_image, "Files"),
        ] {
            let size = image_size(&d, name).await;
            st.images.push(SandboxImage {
                name: name.clone(),
                role: role.into(),
                present: size.is_some(),
                size_bytes: size.flatten(),
            });
        }
        st
    }

    /// Docker, when its engine answers; else why not.
    pub async fn ready(&self) -> Result<Docker, SandboxError> {
        let cached = {
            let (s, at) = &*lock(&self.inner.status);
            s.state == DockerState::Ready && at.is_some_and(|t| t.elapsed() < STATUS_TTL)
        };
        let st = self.docker_status(!cached).await;
        match st.state {
            DockerState::Ready => self.docker(),
            DockerState::Missing => Err(SandboxError::DockerMissing),
            _ => Err(SandboxError::DockerStopped(
                st.message
                    .unwrap_or_else(|| "its engine isn't answering".into()),
            )),
        }
    }

    /// Downloads an image (one download at a time). The status shows it meanwhile.
    pub async fn pull(&self, image: &str) -> Result<(), SandboxError> {
        let d = self.ready().await?;
        let _one = self.inner.pulls.lock().await;
        if image_size(&d, image).await.is_some() {
            return Ok(());
        }
        lock(&self.inner.status).0.pulling = Some(image.to_owned());
        self.changed(None);
        tracing::info!("sandbox: downloading {image}");
        let r = d.run(["pull", image], PULL).await;
        lock(&self.inner.status).0.pulling = None;
        self.docker_status(true).await;
        self.changed(None);
        let o = r?;
        if o.ok() {
            Ok(())
        } else if o.timed_out {
            Err(SandboxError::Docker(format!(
                "Downloading {image} took longer than an hour and was stopped."
            )))
        } else {
            Err(SandboxError::Docker(format!(
                "{image} couldn't be downloaded: {}",
                o.error_text()
            )))
        }
    }

    /// Downloads both images when missing (Settings → Sandbox → Download images).
    pub async fn prepare(&self) -> Result<(), SandboxError> {
        let s = self.settings();
        self.pull(&s.image).await?;
        self.pull(&s.files_image).await
    }

    // ---- One chat's sandbox -----------------------------------------------------------------

    fn chat_lock(&self, key: &str) -> Arc<tokio::sync::Mutex<()>> {
        lock(&self.inner.locks)
            .entry(key.to_owned())
            .or_default()
            .clone()
    }

    pub(crate) fn touch(&self, id: &str) {
        lock(&self.inner.chats)
            .entry(id.to_owned())
            .or_default()
            .last_used = Some(Instant::now());
    }

    fn busy(&self, id: &str) -> bool {
        lock(&self.inner.chats)
            .get(id)
            .is_some_and(|c| !c.commands.is_empty())
    }

    async fn inspect(&self, d: &Docker, name: &str) -> Result<Option<Existing>, SandboxError> {
        let o = d
            .run(
                [
                    "container",
                    "inspect",
                    "--format",
                    "{{.State.Running}}\t{{.Config.Image}}\t{{index .Config.Labels \"oxide.config\"}}\t{{index .Config.Labels \"oxide.workspace\"}}",
                    name,
                ],
                QUICK,
            )
            .await?;
        if !o.ok() {
            let err = o.error_text();
            if err.to_ascii_lowercase().contains("no such") {
                return Ok(None);
            }
            return Err(SandboxError::Docker(err));
        }
        let text = o.stdout_text();
        let mut p = text.split('\t');
        let mut next = || {
            p.next()
                .map(str::trim)
                .filter(|v| *v != "<no value>")
                .unwrap_or("")
                .to_owned()
        };
        Ok(Some(Existing {
            running: next() == "true",
            image: next(),
            key: next(),
            workspace: next(),
        }))
    }

    /// Where a new chat's files go, by the current settings.
    fn new_workspace(&self, id: &str, settings: &SandboxSettings) -> Workspace {
        let name = sandbox_name(id, false);
        match settings.storage {
            SandboxStorage::Volume => Workspace::Volume(name),
            SandboxStorage::Folder => {
                let base = if settings.folder.is_empty() {
                    self.inner.root.join("workspaces")
                } else {
                    PathBuf::from(&settings.folder)
                };
                Workspace::Folder(base.join(name))
            }
        }
    }

    async fn ensure_image(&self, d: &Docker, image: &str) -> Result<(), SandboxError> {
        if image_size(d, image).await.is_some() {
            return Ok(());
        }
        self.pull(image).await
    }

    /// Makes sure the chat's container exists, is made with the current settings and runs.
    /// Returns its name. A container made with other settings is replaced; its workspace
    /// (and the files in it) stays.
    pub async fn ensure(&self, id: &str) -> Result<String, SandboxError> {
        let d = self.ready().await?;
        let guard = self.chat_lock(id);
        let _one = guard.lock().await;
        let settings = self.settings();
        let name = sandbox_name(id, false);
        let existing = self.inspect(&d, &name).await?;
        if let Some(e) = &existing {
            if e.key == settings.container_key() || self.busy(id) {
                if !e.running {
                    d.ok(["start", name.as_str()], CREATE).await?;
                    self.changed(Some(id));
                }
                self.touch(id);
                return Ok(name);
            }
            tracing::info!("sandbox {name}: settings changed, making a new container");
            self.close_files(id).await;
            d.ok(["rm", "-f", name.as_str()], QUICK).await?;
        }
        let workspace = existing
            .as_ref()
            .and_then(|e| Workspace::parse(&e.workspace))
            .unwrap_or_else(|| self.new_workspace(id, &settings));
        {
            let mut chats = lock(&self.inner.chats);
            let c = chats.entry(id.to_owned()).or_default();
            c.starting = true;
            c.error = None;
        }
        self.changed(Some(id));
        let made = self.create(&d, id, &name, &workspace, &settings).await;
        {
            let mut chats = lock(&self.inner.chats);
            let c = chats.entry(id.to_owned()).or_default();
            c.starting = false;
            c.error = made.as_ref().err().map(ToString::to_string);
        }
        self.changed(Some(id));
        made?;
        self.touch(id);
        Ok(name)
    }

    async fn create(
        &self,
        d: &Docker,
        id: &str,
        name: &str,
        workspace: &Workspace,
        settings: &SandboxSettings,
    ) -> Result<(), SandboxError> {
        self.ensure_image(d, &settings.image).await?;
        self.prepare_workspace(d, id, workspace).await?;
        let (cpus, memory_mb) = {
            let st = &lock(&self.inner.status).0;
            let cpus = st
                .cpus
                .map_or(settings.cpus, |n| settings.cpus.min(n.max(1)));
            let mem = st.memory_bytes.map_or(settings.memory_mb, |b| {
                let have = u32::try_from(b / (1024 * 1024)).unwrap_or(u32::MAX);
                settings.memory_mb.min(have.max(256))
            });
            (cpus, mem)
        };
        let mut args: Vec<String> = vec![
            "run".into(),
            "-d".into(),
            "--name".into(),
            name.into(),
            "--hostname".into(),
            "sandbox".into(),
            "--label".into(),
            LABEL.into(),
            "--label".into(),
            format!("oxide.chat={id}"),
            "--label".into(),
            format!("oxide.config={}", settings.container_key()),
            "--label".into(),
            format!("oxide.workspace={}", workspace.label()),
            "--cap-drop".into(),
            "ALL".into(),
        ];
        for cap in CAPS {
            args.push("--cap-add".into());
            args.push(cap.into());
        }
        args.extend([
            "--security-opt".into(),
            "no-new-privileges".into(),
            "--pids-limit".into(),
            "1024".into(),
            "--cpus".into(),
            cpus.to_string(),
            "--memory".into(),
            format!("{memory_mb}m"),
            "--memory-swap".into(),
            format!("{memory_mb}m"),
        ]);
        if !settings.network {
            args.push("--network".into());
            args.push("none".into());
        }
        args.extend([
            // A fix written within the same second as the old file, at the same size, would
            // otherwise run Python's stale bytecode (it checks mtime and size only).
            "--env".into(),
            "PYTHONDONTWRITEBYTECODE=1".into(),
            "--mount".into(),
            workspace.mount()?,
            "-w".into(),
            WORKSPACE_DIR.into(),
            "--entrypoint".into(),
            "sh".into(),
            settings.image.clone(),
            "-c".into(),
            script::KEEP_ALIVE.into(),
        ]);
        d.ok(&args, CREATE)
            .await
            .map_err(|e| SandboxError::Docker(format!("The sandbox couldn't be created: {e}")))?;
        Ok(())
    }

    async fn prepare_workspace(
        &self,
        d: &Docker,
        id: &str,
        workspace: &Workspace,
    ) -> Result<(), SandboxError> {
        match workspace {
            Workspace::Volume(v) => {
                d.ok(
                    [
                        "volume",
                        "create",
                        "--label",
                        LABEL,
                        "--label",
                        &format!("oxide.chat={id}"),
                        v.as_str(),
                    ],
                    QUICK,
                )
                .await?;
            }
            Workspace::Folder(p) => tokio::fs::create_dir_all(p).await?,
        }
        Ok(())
    }

    /// The chat's sandbox as the session panel shows it. Never starts anything.
    pub async fn chat(&self, id: &str) -> ChatSandbox {
        let name = sandbox_name(id, false);
        let settings = self.settings();
        let (commands, starting, error) = {
            let chats = lock(&self.inner.chats);
            chats.get(id).map_or((Vec::new(), false, None), |c| {
                (c.commands.clone(), c.starting, c.error.clone())
            })
        };
        let mut out = ChatSandbox {
            conversation_id: id.to_owned(),
            container: name.clone(),
            storage: settings.storage,
            network: settings.network,
            commands,
            error,
            ..ChatSandbox::default()
        };
        let d = match self.ready().await {
            Ok(d) => d,
            Err(e) => {
                out.error = Some(e.to_string());
                return out;
            }
        };
        match self.inspect(&d, &name).await {
            Ok(Some(e)) => {
                out.state = if e.running {
                    SandboxState::Running
                } else {
                    SandboxState::Stopped
                };
                out.image = Some(e.image);
                if let Some(w) = Workspace::parse(&e.workspace) {
                    out.storage = match w {
                        Workspace::Volume(_) => SandboxStorage::Volume,
                        Workspace::Folder(_) => SandboxStorage::Folder,
                    };
                    out.workspace = Some(w.display());
                }
                out.network = !e.key.ends_with("|nonet");
            }
            Ok(None) => {}
            Err(e) => out.error = Some(e.to_string()),
        }
        if starting {
            out.state = SandboxState::Starting;
        }
        out
    }

    /// Commands running in the chat's sandbox now.
    pub fn running_commands(&self, id: &str) -> Vec<SandboxCommand> {
        lock(&self.inner.chats)
            .get(id)
            .map(|c| c.commands.clone())
            .unwrap_or_default()
    }

    /// Runs `command` (bash when the image has it, else sh) in `cwd` inside the chat's
    /// sandbox, starting it first. Past `timeout` the command and everything it started are
    /// killed.
    pub async fn exec(
        &self,
        id: &str,
        command: &str,
        cwd: &str,
        timeout: Duration,
    ) -> Result<ExecResult, SandboxError> {
        let cwd = workspace_path(cwd).map_err(SandboxError::Invalid)?;
        let name = self.ensure(id).await?;
        let d = self.docker()?;
        let run = self.inner.next.fetch_add(1, Ordering::Relaxed).to_string();
        {
            let mut chats = lock(&self.inner.chats);
            let c = chats.entry(id.to_owned()).or_default();
            c.commands.push(SandboxCommand {
                id: run.clone(),
                command: command.to_owned(),
                started_at: now_ms(),
            });
            c.last_used = Some(Instant::now());
        }
        self.changed(Some(id));
        let marker = format!("OXIDE_EXEC={run}");
        let r = d
            .run(
                [
                    "exec",
                    "-e",
                    marker.as_str(),
                    "-w",
                    WORKSPACE_DIR,
                    name.as_str(),
                    "sh",
                    "-c",
                    script::EXEC,
                    "oxide",
                    command,
                    cwd.as_str(),
                ],
                timeout,
            )
            .await;
        if r.as_ref().is_ok_and(|o| o.timed_out) {
            // Killing the docker client leaves the command running inside: end every process
            // that carries this run's marker.
            let _ = d
                .run(
                    [
                        "exec",
                        name.as_str(),
                        "sh",
                        "-c",
                        script::KILL_RUN,
                        "oxide",
                        &run,
                    ],
                    QUICK,
                )
                .await;
        }
        {
            let mut chats = lock(&self.inner.chats);
            if let Some(c) = chats.get_mut(id) {
                c.commands.retain(|x| x.id != run);
                c.last_used = Some(Instant::now());
            }
        }
        self.changed(Some(id));
        let o = r?;
        Ok(ExecResult {
            cwd,
            code: o.code,
            stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
            timed_out: o.timed_out,
            elapsed: o.elapsed,
        })
    }

    /// The contents of a folder in the chat's workspace. `start`: start a stopped sandbox to
    /// look (else `None` when it doesn't run).
    pub async fn list(
        &self,
        id: &str,
        path: &str,
        start: bool,
    ) -> Result<Option<SandboxListing>, SandboxError> {
        let path = workspace_path(path).map_err(SandboxError::Invalid)?;
        let d = self.ready().await?;
        let name = sandbox_name(id, false);
        let running = matches!(self.inspect(&d, &name).await?, Some(e) if e.running);
        if !running {
            if !start {
                return Ok(None);
            }
            self.ensure(id).await?;
        }
        let o = d
            .run(
                [
                    "exec",
                    name.as_str(),
                    "sh",
                    "-c",
                    script::LIST,
                    "oxide",
                    path.as_str(),
                    &MAX_LISTING.to_string(),
                ],
                QUICK,
            )
            .await?;
        if o.code == Some(3) {
            return Err(SandboxError::Invalid(format!(
                "{path} isn't a folder in the sandbox."
            )));
        }
        let text = o.into_result()?;
        Ok(Some(parse_listing(&path, &text)))
    }

    /// Copies a file or folder on this computer into the sandbox. `dest` is a workspace path:
    /// an existing folder (or one ending in `/`) receives it under its own name. Returns the
    /// path it now has in the sandbox.
    pub async fn copy_in(&self, id: &str, host: &Path, dest: &str) -> Result<String, SandboxError> {
        let into_dir = dest.trim().is_empty() || dest.trim_end().ends_with(['/', '\\']);
        let dest = workspace_path(dest).map_err(SandboxError::Invalid)?;
        let host_owned = host.to_path_buf();
        let (bytes, is_dir) = tokio::task::spawn_blocking(move || tree_size(&host_owned))
            .await
            .map_err(|e| SandboxError::Invalid(e.to_string()))??;
        if bytes > MAX_COPY_BYTES {
            return Err(SandboxError::Invalid(format!(
                "That is {} — more than the {} a copy may hold.",
                human(bytes),
                human(MAX_COPY_BYTES)
            )));
        }
        let name = self.ensure(id).await?;
        let d = self.docker()?;
        let dest_is_dir = d
            .run(["exec", name.as_str(), "test", "-d", dest.as_str()], QUICK)
            .await?
            .ok();
        let file_name = host
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "copy".into());
        let target = if into_dir || dest == WORKSPACE_DIR || dest_is_dir {
            format!("{}/{file_name}", dest.trim_end_matches('/'))
        } else {
            dest
        };
        let parent = target
            .rsplit_once('/')
            .map(|(p, _)| p)
            .filter(|p| !p.is_empty())
            .unwrap_or(WORKSPACE_DIR)
            .to_owned();
        d.ok(
            ["exec", name.as_str(), "mkdir", "-p", parent.as_str()],
            QUICK,
        )
        .await?;
        // A folder copied onto an existing folder merges into it (`dir/.`), instead of
        // ending up one level deeper.
        let target_is_dir = d
            .run(
                ["exec", name.as_str(), "test", "-d", target.as_str()],
                QUICK,
            )
            .await?
            .ok();
        let mut src = plain(host);
        if is_dir && target_is_dir {
            src.push_str(if cfg!(windows) { "\\." } else { "/." });
        }
        let o = d
            .run(["cp", src.as_str(), &format!("{name}:{target}")], COPY)
            .await?;
        if !o.ok() {
            return Err(SandboxError::Docker(format!(
                "The copy into the sandbox failed: {}",
                o.error_text()
            )));
        }
        self.touch(id);
        self.changed(Some(id));
        Ok(target)
    }

    /// Copies a file or folder from the sandbox to this computer. `host_dest` is a folder
    /// (the copy keeps its name) or a new path; nothing that exists is overwritten, and
    /// trees with symbolic links are refused. Returns where it went.
    pub async fn copy_out(
        &self,
        id: &str,
        src: &str,
        host_dest: &Path,
    ) -> Result<PathBuf, SandboxError> {
        let src = workspace_path(src).map_err(SandboxError::Invalid)?;
        let name = self.ensure(id).await?;
        let d = self.docker()?;
        let o = d
            .run(
                [
                    "exec",
                    name.as_str(),
                    "sh",
                    "-c",
                    script::CHECK_OUT,
                    "oxide",
                    src.as_str(),
                ],
                QUICK,
            )
            .await?;
        match o.code {
            Some(0) => {}
            Some(3) => {
                return Err(SandboxError::Invalid(format!(
                    "{src} doesn't exist in the sandbox."
                )));
            }
            Some(4) => {
                return Err(SandboxError::Invalid(format!(
                    "{src} contains symbolic links, which aren't copied out of the sandbox. \
                     Replace them with the files they point to, or copy the parts without links."
                )));
            }
            _ => return Err(SandboxError::Docker(o.error_text())),
        }
        let kb: u64 = o.stdout_text().trim().parse().unwrap_or(0);
        if kb.saturating_mul(1024) > MAX_COPY_BYTES {
            return Err(SandboxError::Invalid(format!(
                "{src} is {} — more than the {} a copy may hold.",
                human(kb * 1024),
                human(MAX_COPY_BYTES)
            )));
        }
        let target = if host_dest.is_dir() {
            host_dest.join(base_name(&src))
        } else {
            host_dest.to_path_buf()
        };
        if tokio::fs::symlink_metadata(&target).await.is_ok() {
            return Err(SandboxError::Invalid(format!(
                "{} already exists; nothing was overwritten. Choose another name or folder.",
                plain(&target)
            )));
        }
        if let Some(parent) = target.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let o = d
            .run(
                ["cp", &format!("{name}:{src}"), plain(&target).as_str()],
                COPY,
            )
            .await?;
        if !o.ok() {
            return Err(SandboxError::Docker(format!(
                "The copy out of the sandbox failed: {}",
                o.error_text()
            )));
        }
        self.touch(id);
        Ok(target)
    }

    /// Stops the chat's sandbox (its files stay).
    pub async fn stop(&self, id: &str) -> Result<(), SandboxError> {
        self.close_files(id).await;
        let d = self.docker()?;
        let name = sandbox_name(id, false);
        let files = sandbox_name(id, true);
        let _ = d.run(["rm", "-f", files.as_str()], QUICK).await;
        let o = d.run(["stop", "-t", "2", name.as_str()], QUICK).await?;
        self.changed(Some(id));
        if o.ok() || o.error_text().to_ascii_lowercase().contains("no such") {
            Ok(())
        } else {
            Err(SandboxError::Docker(o.error_text()))
        }
    }

    /// Removes the chat's sandbox and its files. It starts empty the next time it's used.
    pub async fn reset(&self, id: &str) -> Result<(), SandboxError> {
        if self.busy(id) {
            return Err(SandboxError::Invalid(
                "A command is still running in this sandbox. Try again when it has finished."
                    .into(),
            ));
        }
        let d = self.docker()?;
        let guard = self.chat_lock(id);
        let _one = guard.lock().await;
        self.close_files(id).await;
        let name = sandbox_name(id, false);
        let existing = self.inspect(&d, &name).await.ok().flatten();
        let o = d
            .run(
                ["rm", "-f", name.as_str(), sandbox_name(id, true).as_str()],
                QUICK,
            )
            .await?;
        if !o.ok() && !o.error_text().to_ascii_lowercase().contains("no such") {
            return Err(SandboxError::Docker(o.error_text()));
        }
        let settings = self.settings();
        let workspace = existing
            .and_then(|e| Workspace::parse(&e.workspace))
            .unwrap_or_else(|| self.new_workspace(id, &settings));
        match &workspace {
            Workspace::Volume(v) => {
                let o = d.run(["volume", "rm", "-f", v.as_str()], QUICK).await?;
                if !o.ok() && !o.error_text().to_ascii_lowercase().contains("no such") {
                    return Err(SandboxError::Docker(o.error_text()));
                }
            }
            // Only a folder Oxide named for this chat is removed.
            Workspace::Folder(p) => {
                if p.file_name().and_then(|n| n.to_str()) == Some(name.as_str()) && p.exists() {
                    tokio::fs::remove_dir_all(p).await?;
                }
            }
        }
        lock(&self.inner.chats).remove(id);
        self.changed(Some(id));
        Ok(())
    }

    /// The chat was deleted: removes its sandbox and files, quietly.
    pub async fn forget(&self, id: &str) {
        if self.find_docker().is_none() {
            return;
        }
        if let Err(e) = self.reset(id).await {
            tracing::warn!("couldn't remove a deleted chat's sandbox: {e}");
        }
        lock(&self.inner.locks).retain(|k, _| k != id && *k != format!("files:{id}"));
    }

    /// Stops sandboxes idle for longer than the setting (and, the first time, ones left
    /// running by an earlier run of Oxide).
    pub async fn stop_idle(&self, include_orphans: bool) {
        let idle = Duration::from_secs(u64::from(self.settings().idle_minutes) * 60);
        let due: Vec<String> = lock(&self.inner.chats)
            .iter()
            .filter(|(_, c)| {
                c.commands.is_empty()
                    && !c.starting
                    && c.last_used.is_some_and(|t| t.elapsed() >= idle)
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in due {
            tracing::debug!("sandbox for {id} is idle: stopping it");
            if let Err(e) = self.stop(&id).await {
                tracing::warn!("couldn't stop an idle sandbox: {e}");
            }
            if let Some(c) = lock(&self.inner.chats).get_mut(&id) {
                c.last_used = None;
            }
        }
        if include_orphans {
            self.stop_untracked().await;
        }
    }

    /// Stops running sandbox containers this run of Oxide hasn't used.
    async fn stop_untracked(&self) {
        let Ok(d) = self.ready().await else { return };
        let Ok(text) = d
            .ok(
                [
                    "ps",
                    "--filter",
                    &format!("label={LABEL}"),
                    "--format",
                    "{{.Names}}\t{{.Label \"oxide.chat\"}}",
                ],
                QUICK,
            )
            .await
        else {
            return;
        };
        let used: Vec<String> = lock(&self.inner.chats)
            .iter()
            .filter(|(_, c)| c.last_used.is_some() || !c.commands.is_empty())
            .map(|(id, _)| id.clone())
            .collect();
        let names: Vec<String> = text
            .lines()
            .filter_map(|l| l.split_once('\t'))
            .filter(|(_, chat)| !used.iter().any(|u| u == chat.trim()))
            .map(|(n, _)| n.trim().to_owned())
            .filter(|n| !n.is_empty())
            .collect();
        if names.is_empty() {
            return;
        }
        tracing::info!("stopping {} sandbox(es) left running", names.len());
        let mut args = vec!["stop".to_owned(), "-t".into(), "2".into()];
        args.extend(names);
        let _ = d.run(&args, QUICK).await;
    }

    /// Checks for idle sandboxes every minute (on the current runtime).
    pub fn start_reaper(&self) {
        let me = self.clone();
        let Ok(rt) = tokio::runtime::Handle::try_current() else {
            tracing::warn!("sandbox: no runtime for the idle check");
            return;
        };
        let task = rt.spawn(async move {
            let mut first = true;
            loop {
                tokio::time::sleep(REAP_EVERY).await;
                if me.find_docker().is_some() {
                    me.stop_idle(first).await;
                    first = false;
                }
            }
        });
        if let Some(old) = lock(&self.inner.reaper).replace(task) {
            old.abort();
        }
    }

    /// Oxide is quitting: stops every sandbox (files stay) and the file servers.
    pub async fn shutdown(&self) {
        if let Some(t) = lock(&self.inner.reaper).take() {
            t.abort();
        }
        let ids: Vec<String> = lock(&self.inner.chats).keys().cloned().collect();
        for id in &ids {
            self.close_files(id).await;
        }
        let Some(d) = self.find_docker() else { return };
        let Ok(text) = d
            .ok(["ps", "-q", "--filter", &format!("label={LABEL}")], QUICK)
            .await
        else {
            return;
        };
        let mut args = vec!["stop".to_owned(), "-t".into(), "2".into()];
        args.extend(
            text.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_owned),
        );
        if args.len() > 3 {
            let _ = d.run(&args, QUICK).await;
        }
    }

    // ---- The file server's connection (see files.rs) --------------------------------------

    pub(crate) fn files_client(&self, id: &str) -> Option<Arc<McpClient>> {
        lock(&self.inner.chats)
            .get(id)
            .and_then(|c| c.files.clone())
            .filter(|c| c.closed().borrow().is_none())
    }

    pub(crate) fn set_files_client(&self, id: &str, client: Option<Arc<McpClient>>) {
        lock(&self.inner.chats)
            .entry(id.to_owned())
            .or_default()
            .files = client;
    }

    pub(crate) fn files_lock(&self, id: &str) -> Arc<tokio::sync::Mutex<()>> {
        self.chat_lock(&format!("files:{id}"))
    }

    pub(crate) async fn workspace_mount(&self, id: &str) -> Result<String, SandboxError> {
        let d = self.docker()?;
        let e = self
            .inspect(&d, &sandbox_name(id, false))
            .await?
            .ok_or_else(|| SandboxError::Invalid("The sandbox doesn't exist yet.".into()))?;
        Workspace::parse(&e.workspace)
            .ok_or_else(|| SandboxError::Invalid("The sandbox has no workspace label.".into()))?
            .mount()
    }

    pub(crate) async fn ensure_files_image(&self) -> Result<Docker, SandboxError> {
        let d = self.ready().await?;
        let image = self.settings().files_image;
        self.ensure_image(&d, &image).await?;
        Ok(d)
    }

    async fn close_files(&self, id: &str) {
        let client = lock(&self.inner.chats)
            .get_mut(id)
            .and_then(|c| c.files.take());
        if let Some(c) = client {
            c.close().await;
        }
    }
}

/// The image's size when it is downloaded (`Some(None)`: present, size unknown); `None`
/// when it isn't.
pub(crate) async fn image_size(d: &Docker, image: &str) -> Option<Option<u64>> {
    let o = d
        .run(["image", "inspect", "--format", "{{.Size}}", image], QUICK)
        .await
        .ok()?;
    o.ok().then(|| o.stdout_text().trim().parse().ok())
}

/// Reads `kind<TAB>size<TAB>name` lines (and `+` when cut off).
fn parse_listing(path: &str, text: &str) -> SandboxListing {
    let mut entries = Vec::new();
    let mut truncated = false;
    for line in text.lines() {
        if line == "+" {
            truncated = true;
            continue;
        }
        let mut p = line.splitn(3, '\t');
        let (Some(kind), Some(size), Some(name)) = (p.next(), p.next(), p.next()) else {
            continue;
        };
        if name.is_empty() || name == "." || name == ".." {
            continue;
        }
        entries.push(SandboxFile {
            name: name.to_owned(),
            path: if path == WORKSPACE_DIR {
                format!("{WORKSPACE_DIR}/{name}")
            } else {
                format!("{path}/{name}")
            },
            dir: kind == "d",
            link: kind == "l",
            size: if kind == "f" {
                size.trim().parse().ok()
            } else {
                None
            },
        });
    }
    entries.sort_by(|a, b| {
        b.dir
            .cmp(&a.dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    SandboxListing {
        path: path.to_owned(),
        entries,
        truncated,
    }
}

/// Bytes in a file or folder (links not followed), and whether it is a folder.
fn tree_size(p: &Path) -> Result<(u64, bool), SandboxError> {
    let meta = std::fs::symlink_metadata(p)
        .map_err(|e| SandboxError::Invalid(format!("{} can't be read: {e}", plain(p))))?;
    if !meta.is_dir() {
        return Ok((meta.len(), false));
    }
    let mut total = 0u64;
    let mut stack = vec![p.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let m = entry.metadata()?;
            if m.is_dir() {
                stack.push(entry.path());
            } else {
                total = total.saturating_add(m.len());
            }
            if total > MAX_COPY_BYTES {
                return Ok((total, true));
            }
        }
    }
    Ok((total, true))
}

/// `2.0 GB`, `512 KB`.
pub(crate) fn human(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["bytes", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{bytes} bytes")
    } else {
        format!("{v:.1} {}", UNITS[u])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_labels_round_trip() {
        let v = Workspace::Volume("oxide-sandbox-a".into());
        assert_eq!(Workspace::parse(&v.label()), Some(v.clone()));
        assert_eq!(
            v.mount().expect("mount"),
            "type=volume,src=oxide-sandbox-a,dst=/workspace"
        );
        let f = Workspace::Folder(PathBuf::from("/data/oxide-sandbox-a"));
        assert_eq!(Workspace::parse(&f.label()), Some(f.clone()));
        assert!(Workspace::Folder(PathBuf::from("/a,b")).mount().is_err());
        assert_eq!(Workspace::parse("volume:"), None);
        assert_eq!(Workspace::parse("other"), None);
    }

    #[test]
    fn verbatim_prefixes_are_dropped() {
        assert_eq!(plain(Path::new(r"\\?\C:\Users\a")), r"C:\Users\a");
        assert_eq!(plain(Path::new(r"\\?\UNC\srv\share")), r"\\srv\share");
        assert_eq!(plain(Path::new("/home/a")), "/home/a");
    }

    #[test]
    fn listings_are_parsed_and_sorted() {
        let l = parse_listing(
            "/workspace",
            "f\t12\tb.txt\nd\t\tsrc\nl\t\tlink\nbroken line\nf\t3\tA.md\n+\n",
        );
        assert!(l.truncated);
        let names: Vec<_> = l.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["src", "A.md", "b.txt", "link"]);
        assert_eq!(l.entries[2].size, Some(12));
        assert_eq!(l.entries[2].path, "/workspace/b.txt");
        assert!(l.entries[3].link);
        let l = parse_listing("/workspace/src", "f\t1\tx\n");
        assert_eq!(l.entries[0].path, "/workspace/src/x");
    }

    #[test]
    fn sizes_are_readable() {
        assert_eq!(human(10), "10 bytes");
        assert_eq!(human(2 * 1024 * 1024 * 1024), "2.0 GB");
    }

    #[test]
    fn tree_sizes_add_up() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir(dir.path().join("a")).expect("mkdir");
        std::fs::write(dir.path().join("a/x"), [0u8; 10]).expect("write");
        std::fs::write(dir.path().join("y"), [0u8; 5]).expect("write");
        assert_eq!(tree_size(dir.path()).expect("size"), (15, true));
        assert_eq!(tree_size(&dir.path().join("y")).expect("size"), (5, false));
        assert!(tree_size(&dir.path().join("nope")).is_err());
    }

    #[tokio::test]
    async fn without_docker_everything_says_so() {
        let dir = tempfile::tempdir().expect("tempdir");
        let s = Sandboxes::with_docker(
            dir.path().into(),
            None,
            Arc::new(NoSink),
            Some(dir.path().join("no-docker")),
        );
        let st = s.docker_status(true).await;
        assert_eq!(st.state, DockerState::Missing);
        assert!(st.message.is_some());
        assert_eq!(st.install_url, DOCKER_INSTALL_URL);
        assert!(matches!(
            s.exec("c", "ls", "", Duration::from_secs(1)).await,
            Err(SandboxError::DockerMissing)
        ));
        let chat = s.chat("c").await;
        assert_eq!(chat.state, SandboxState::None);
        assert!(chat.error.is_some());
        s.forget("c").await;
    }
}
