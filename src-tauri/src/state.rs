//! State shared by all commands.

use std::path::PathBuf;
use std::sync::Arc;

use oxide_agent::{
    AttachmentStore, ChatService, ComputerUse, EventSink, McpTools, ModelRegistry, Scheduler,
    SchedulerSink, Skills, Tools,
};
use oxide_bridge::{Bridge, BridgeSink};
use oxide_mcp::{Extensions, ManagerOptions, ManagerSink, McpManager};
use oxide_pyenv::{BrowserAgents, BrowserSink, PythonConfig, PythonEnv, PythonSink};
use oxide_sandbox::{SandboxSink, Sandboxes};
use oxide_store::{KeyringSecrets, SecretStore, Store};

pub struct AppState {
    pub store: Store,
    pub secrets: Arc<dyn SecretStore>,
    /// Models per endpoint, cached.
    pub models: Arc<ModelRegistry>,
    /// Writes replies and streams them to `sink`.
    pub chat: Arc<ChatService>,
    /// Attached files, stored under `<data dir>/attachments`.
    pub attachments: AttachmentStore,
    /// MCP servers from `<config dir>/oxide_config.json`.
    pub mcp: McpManager,
    /// Desktop extensions, unpacked under `<data dir>/extensions`; their servers run in `mcp`.
    pub extensions: Extensions,
    /// Skills under `<data dir>/skills`.
    pub skills: Skills,
    /// The managed Python environment under `<data dir>/python`.
    pub python: PythonEnv,
    /// The Docker sandbox: a workspace container per chat.
    pub sandboxes: Sandboxes,
    /// The browser agent: a server (and browser) per chat, in the managed Python.
    pub browser: BrowserAgents,
    /// The Chrome extension's bridge (a WebSocket on 127.0.0.1 while Settings → Chrome is on).
    pub chrome: Bridge,
    /// Scheduled tasks: runs them in new chats while Oxide is open or in the tray.
    pub scheduler: Arc<Scheduler>,
    /// Computer use (Windows only for now): `None` where it isn't supported.
    pub computer: Option<ComputerUse>,
    /// The control bar and Oxide's window around computer use.
    pub computer_bar: Arc<crate::computer::TauriComputerSink>,
    /// Oxide's data, config and log folders (Settings → Data).
    pub dirs: oxide_agent::data::DataDirs,
}

/// Where Oxide looks for its `uv`: next to the app (the bundled sidecar), then the folder
/// `npm run fetch-uv` fills in a development build.
fn uv_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        dirs.push(dir.to_path_buf());
    }
    dirs.push(PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/binaries"
    )));
    dirs
}

/// The built extension users load unpacked. Release builds: the installer's copy in the
/// app's resources (`bundle.resources` in `tauri.conf.json`), else next to the app. Debug
/// builds: the repo's `extension/dist` first, so a pairing made with that folder keeps
/// working (an unpacked extension's id comes from its folder). `None` until it's built.
fn extension_dir(resources: Option<&std::path::Path>) -> Option<String> {
    let mut packaged = Vec::new();
    if let Some(dir) = resources {
        packaged.push(dir.join("extension"));
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        packaged.push(dir.join("extension"));
    }
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|r| r.join("extension").join("dist"));
    let dirs: Vec<PathBuf> = if cfg!(debug_assertions) {
        repo.into_iter().chain(packaged).collect()
    } else {
        packaged.into_iter().chain(repo).collect()
    };
    dirs.into_iter()
        .find(|d| d.join("manifest.json").is_file())
        .map(|d| d.to_string_lossy().into_owned())
}

/// Where the app keeps its files.
pub struct AppDirs {
    pub data: PathBuf,
    pub config: PathBuf,
    pub logs: PathBuf,
    /// The app's bundled resources (the Chrome extension), when Tauri knows them.
    pub resources: Option<PathBuf>,
}

impl AppDirs {
    /// The folders "Export all" and "Delete all" work on.
    pub fn data_dirs(&self) -> oxide_agent::data::DataDirs {
        oxide_agent::data::DataDirs {
            data: self.data.clone(),
            config: self.config.clone(),
            logs: self.logs.clone(),
        }
    }
}

/// The app-side pieces the MCP manager needs.
pub struct McpParts {
    pub sink: Arc<dyn ManagerSink>,
    pub agent: Arc<dyn oxide_mcp::UserAgent>,
    pub tokens: Arc<dyn oxide_mcp::oauth::TokenStore>,
}

impl AppState {
    /// Opens the database in `data_dir` (creating it) and uses the OS keychain for secrets,
    /// with entries grouped under `service` (the app identifier). Reply events go to `sink`.
    // One sink per subsystem (chat, Python, sandbox, browser, Chrome, scheduler, computer use)
    // plus the MCP parts.
    #[allow(clippy::too_many_arguments)]
    pub async fn open(
        dirs: &AppDirs,
        service: &str,
        sink: Arc<dyn EventSink>,
        python_sink: Arc<dyn PythonSink>,
        sandbox_sink: Arc<dyn SandboxSink>,
        browser_sink: Arc<dyn BrowserSink>,
        chrome_sink: Arc<dyn BridgeSink>,
        schedule_sink: Arc<dyn SchedulerSink>,
        computer_bar: Arc<crate::computer::TauriComputerSink>,
        mcp: impl FnOnce(&Arc<ModelRegistry>, &Arc<dyn SecretStore>) -> McpParts,
    ) -> anyhow::Result<Self> {
        let data_dir = dirs.data.as_path();
        tokio::fs::create_dir_all(data_dir).await?;
        let db = data_dir.join(oxide_store::DB_FILE_NAME);
        tracing::info!("database: {}", db.display());
        let store = Store::open(&db).await?;
        let secrets: Arc<dyn SecretStore> = Arc::new(KeyringSecrets::new(service));
        let models = Arc::new(ModelRegistry::new(store.clone(), secrets.clone()));
        let attachments = AttachmentStore::new(data_dir.join("attachments"));
        let parts = mcp(&models, &secrets);
        let mcp = McpManager::new(
            dirs.config.join(oxide_mcp::config::CONFIG_FILE_NAME),
            Some(dirs.logs.clone()),
            parts.sink,
            parts.agent,
            parts.tokens.clone(),
            ManagerOptions::default(),
        );
        let extensions = Extensions::new(data_dir.join("extensions"), mcp.clone(), parts.tokens);
        let skills = Skills::new(data_dir.join("skills"));
        let mut python_config = PythonConfig::new(data_dir.join("python"));
        python_config.uv_dirs = uv_dirs();
        let python = PythonEnv::new(python_config, python_sink);
        // Python extensions run through the same uv.
        extensions.set_uv(python.find_uv());
        let sandboxes = Sandboxes::new(
            data_dir.join("sandboxes"),
            Some(dirs.logs.clone()),
            sandbox_sink,
        );
        let browser = BrowserAgents::new(python.clone(), browser_sink, Some(dirs.logs.clone()));
        let chrome = Bridge::open(data_dir.join("chrome.json"), secrets.clone(), chrome_sink).await;
        chrome.set_extension_dir(extension_dir(dirs.resources.as_deref()));
        match store.app_settings().await {
            Ok(s) => {
                computer_bar.set_display(s.computer.display.clone());
                sandboxes.configure(&s.sandbox);
                browser.configure(&s.browser);
                chrome.configure(&s.chrome).await;
            }
            Err(e) => tracing::warn!("settings: {e}"),
        }
        // Models get the running MCP servers' tools plus Oxide's web tools.
        let tools = Tools::new(
            Some(Arc::new(mcp.clone()) as Arc<dyn McpTools>),
            secrets.clone(),
            Some(attachments.clone()),
        )
        .with_skills(skills.clone())
        .with_python(python.clone())
        .with_sandboxes(sandboxes.clone())
        .with_browser(browser.clone())
        .with_chrome(chrome.clone());
        // Computer use: the real screen, mouse and keyboard, where Oxide can control them.
        let computer = oxide_computer::Desktop::supported().then(|| {
            ComputerUse::new(
                Arc::new(crate::computer::DesktopControl(
                    oxide_computer::Desktop::new(),
                )),
                computer_bar.clone(),
            )
            .with_folders(computer_bar.known_folders())
        });
        let tools = match &computer {
            Some(c) => tools.with_computer(c.clone()),
            None => tools,
        };
        let chat = ChatService::new(store.clone(), models.clone(), sink, tools);
        let scheduler = Scheduler::new(
            store.clone(),
            chat.clone(),
            schedule_sink,
            Arc::new(crate::zone::SystemZone),
        );
        Ok(Self {
            store,
            secrets,
            models,
            chat,
            attachments,
            mcp,
            extensions,
            skills,
            python,
            sandboxes,
            browser,
            chrome,
            scheduler,
            computer,
            computer_bar,
            dirs: dirs.data_dirs(),
        })
    }
}
