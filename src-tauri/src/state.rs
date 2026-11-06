//! State shared by all commands.

use std::path::PathBuf;
use std::sync::Arc;

use oxide_agent::{AttachmentStore, ChatService, EventSink, McpTools, ModelRegistry, Tools};
use oxide_mcp::{ManagerOptions, ManagerSink, McpManager};
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
}

/// Where the app keeps its files.
pub struct AppDirs {
    pub data: PathBuf,
    pub config: PathBuf,
    pub logs: PathBuf,
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
    pub async fn open(
        dirs: &AppDirs,
        service: &str,
        sink: Arc<dyn EventSink>,
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
            parts.tokens,
            ManagerOptions::default(),
        );
        // Models get the running MCP servers' tools plus Oxide's web tools.
        let tools = Tools::new(
            Some(Arc::new(mcp.clone()) as Arc<dyn McpTools>),
            secrets.clone(),
            Some(attachments.clone()),
        );
        let chat = ChatService::new(store.clone(), models.clone(), sink, tools);
        Ok(Self {
            store,
            secrets,
            models,
            chat,
            attachments,
            mcp,
        })
    }
}
