//! State shared by all commands.

use std::path::Path;
use std::sync::Arc;

use oxide_store::{KeyringSecrets, SecretStore, Store};

pub struct AppState {
    pub store: Store,
    pub secrets: Arc<dyn SecretStore>,
}

impl AppState {
    /// Opens the database in `data_dir` (creating it) and uses the OS keychain for secrets,
    /// with entries grouped under `service` (the app identifier).
    pub async fn open(data_dir: &Path, service: &str) -> anyhow::Result<Self> {
        tokio::fs::create_dir_all(data_dir).await?;
        let db = data_dir.join(oxide_store::DB_FILE_NAME);
        tracing::info!("database: {}", db.display());
        let store = Store::open(&db).await?;
        Ok(Self {
            store,
            secrets: Arc::new(KeyringSecrets::new(service)),
        })
    }
}
