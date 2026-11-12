//! SQLite persistence for Oxide: repositories, migrations and full-text search, plus the
//! keychain-backed secret store for API keys.
//!
//! One [`Store`] wraps a connection pool. Repository methods live in the submodules as
//! `impl Store` blocks and return `oxide-core` types.

mod artifacts;
mod chat_tools;
mod compactions;
mod conversations;
mod knowledge;
mod memory;
mod messages;
mod probes;
mod projects;
mod providers;
mod search;
pub mod secrets;
mod settings;
mod tool_log;
mod workspace;

use std::path::Path;
use std::str::FromStr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};

pub use artifacts::NewArtifactVersion;
pub use knowledge::{CHUNK_CHARS, MAX_FILE_CHARS, NewKnowledgeFile, any_word_query, chunk_text};
pub use messages::NewReply;
pub use search::fts_query;
pub use secrets::{
    KeyringSecrets, MemorySecrets, Secret, SecretError, SecretStore, delete_large, get_large,
    set_large,
};
pub use tool_log::NewToolUsage;
pub use workspace::folder_name;

/// File name of the database inside the app data directory.
pub const DB_FILE_NAME: &str = "oxide.db";

/// Errors from the store.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("{what} not found")]
    NotFound { what: &'static str },
    #[error("invalid input: {0}")]
    Invalid(String),
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
    #[error("database migration failed: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
    #[error("stored data is corrupt: {0}")]
    Corrupt(String),
    #[error(transparent)]
    Secret(#[from] SecretError),
    #[error("background task failed: {0}")]
    Join(#[from] tokio::task::JoinError),
}

pub type Result<T, E = StoreError> = std::result::Result<T, E>;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// Handle to the database. Cheap to clone (the pool is reference-counted).
#[derive(Debug, Clone)]
pub struct Store {
    pool: SqlitePool,
}

impl Store {
    /// Opens (creating if needed) the database file at `path` and runs pending migrations.
    pub async fn open(path: &Path) -> Result<Self> {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .foreign_keys(true)
            .busy_timeout(Duration::from_secs(5));
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(options)
            .await?;
        Self::with_pool(pool).await
    }

    /// A private in-memory database (tests, previews). Single connection: every connection
    /// to `:memory:` would otherwise see its own empty database.
    pub async fn open_in_memory() -> Result<Self> {
        let options = SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .idle_timeout(None)
            .max_lifetime(None)
            .connect_with(options)
            .await?;
        Self::with_pool(pool).await
    }

    async fn with_pool(pool: SqlitePool) -> Result<Self> {
        MIGRATOR.run(&pool).await?;
        Ok(Self { pool })
    }

    /// Closes the pool, waiting for connections to finish.
    pub async fn close(&self) {
        self.pool.close().await;
    }
}

/// Current Unix time in milliseconds.
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// A new time-ordered id (UUIDv7), the form every stored id takes.
pub fn new_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

#[cfg(test)]
pub(crate) mod test_util {
    use super::Store;

    pub async fn store() -> Store {
        Store::open_in_memory()
            .await
            .expect("in-memory store opens")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxide_core::StartConversation;

    #[tokio::test]
    async fn data_survives_reopening_the_file() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join(DB_FILE_NAME);

        let id = {
            let store = Store::open(&path).await.expect("opens");
            let (c, _) = store
                .start_conversation(StartConversation {
                    text: "Persist me".into(),
                    model: "anthropic/sonnet".into(),
                    project_id: None,
                    attachments: Vec::new(),
                    folders: vec![],
                    tools: None,
                })
                .await
                .expect("creates");
            store
                .rename_conversation(&c.id, "Renamed")
                .await
                .expect("renames");
            store.set_starred(&c.id, true).await.expect("stars");
            store.close().await;
            c.id
        };

        let store = Store::open(&path).await.expect("reopens");
        let c = store.get_conversation(&id).await.expect("loads");
        assert_eq!(c.title, "Renamed");
        assert!(c.starred);
        let hits = store.search("persist", 10).await.expect("searches");
        assert_eq!(hits.len(), 1);
        store.close().await;
    }

    #[tokio::test]
    async fn migrations_are_idempotent() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join(DB_FILE_NAME);
        Store::open(&path).await.expect("first open").close().await;
        let store = Store::open(&path).await.expect("second open");
        assert_eq!(store.list_providers().await.expect("lists").len(), 4);
        store.close().await;
    }
}
