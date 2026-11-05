//! State shared by all commands.

use std::path::Path;
use std::sync::Arc;

use oxide_agent::{AttachmentStore, ChatService, EventSink, ModelRegistry};
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
}

impl AppState {
    /// Opens the database in `data_dir` (creating it) and uses the OS keychain for secrets,
    /// with entries grouped under `service` (the app identifier). Reply events go to `sink`.
    pub async fn open(
        data_dir: &Path,
        service: &str,
        sink: Arc<dyn EventSink>,
    ) -> anyhow::Result<Self> {
        tokio::fs::create_dir_all(data_dir).await?;
        let db = data_dir.join(oxide_store::DB_FILE_NAME);
        tracing::info!("database: {}", db.display());
        let store = Store::open(&db).await?;
        let secrets: Arc<dyn SecretStore> = Arc::new(KeyringSecrets::new(service));
        let models = Arc::new(ModelRegistry::new(store.clone(), secrets.clone()));
        let chat = ChatService::new(store.clone(), models.clone(), sink);
        let attachments = AttachmentStore::new(data_dir.join("attachments"));
        Ok(Self {
            store,
            secrets,
            models,
            chat,
            attachments,
        })
    }
}
