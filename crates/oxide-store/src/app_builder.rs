//! The Utility App builder's chats and checkpoints (Phase 21).

use oxide_core::{AppCheckpoint, CheckpointKind, MAX_APP_CHECKPOINTS, Message};
use sqlx::Row;

use crate::{Result, Store, StoreError, new_id, now_ms};

impl Store {
    /// The builder chat of an app, if it has one.
    pub async fn builder_chat(&self, app_id: &str) -> Result<Option<String>> {
        Ok(
            sqlx::query_scalar("SELECT conversation_id FROM app_builder_chats WHERE app_id = ?")
                .bind(app_id)
                .fetch_optional(&self.pool)
                .await?,
        )
    }

    /// The app a builder chat builds (`None`: an ordinary chat).
    pub async fn builder_app_of(&self, conversation_id: &str) -> Result<Option<String>> {
        Ok(
            sqlx::query_scalar("SELECT app_id FROM app_builder_chats WHERE conversation_id = ?")
                .bind(conversation_id)
                .fetch_optional(&self.pool)
                .await?,
        )
    }

    /// Makes an app's builder chat: an empty chat that the chat list and search leave out.
    /// Returns the existing one when the app has one.
    pub async fn create_builder_chat(
        &self,
        app_id: &str,
        title: &str,
        model: &str,
    ) -> Result<String> {
        if let Some(id) = self.builder_chat(app_id).await? {
            return Ok(id);
        }
        let model = model.trim();
        if model.is_empty() {
            return Err(StoreError::Invalid("a model is required".into()));
        }
        let id = new_id();
        let now = now_ms();
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO conversations (id, title, project_id, model, starred, created_at, updated_at)
             VALUES (?, ?, NULL, ?, 0, ?, ?)",
        )
        .bind(&id)
        .bind(title)
        .bind(model)
        .bind(now)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        sqlx::query("INSERT INTO app_builder_chats (app_id, conversation_id) VALUES (?, ?)")
            .bind(app_id)
            .bind(&id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(id)
    }

    /// The user message a builder reply answers (for its checkpoint's label).
    pub async fn last_user_text(&self, conversation_id: &str) -> Result<Option<String>> {
        let messages: Vec<Message> = self.list_messages(conversation_id).await?;
        Ok(messages
            .iter()
            .rev()
            .find(|m| m.role == oxide_core::Role::User)
            .map(|m| {
                m.content
                    .iter()
                    .filter_map(|b| match b {
                        oxide_core::ContentBlock::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            }))
    }

    /// Adds a checkpoint unless the newest one has the same digest. `files` is the JSON of
    /// the draft's files. Keeps the newest [`MAX_APP_CHECKPOINTS`].
    pub async fn add_app_checkpoint(
        &self,
        app_id: &str,
        label: &str,
        kind: CheckpointKind,
        file_count: u32,
        files: &str,
        digest: &str,
    ) -> Result<Option<AppCheckpoint>> {
        let newest: Option<String> = sqlx::query_scalar(
            "SELECT digest FROM app_checkpoints WHERE app_id = ? ORDER BY seq DESC LIMIT 1",
        )
        .bind(app_id)
        .fetch_optional(&self.pool)
        .await?;
        if newest.as_deref() == Some(digest) {
            return Ok(None);
        }
        let cp = AppCheckpoint {
            id: new_id(),
            app_id: app_id.to_owned(),
            label: label.chars().take(200).collect(),
            kind,
            files: file_count,
            created_at: now_ms(),
        };
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO app_checkpoints
                 (id, app_id, label, kind, file_count, files, digest, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&cp.id)
        .bind(app_id)
        .bind(&cp.label)
        .bind(kind.as_str())
        .bind(i64::from(file_count))
        .bind(files)
        .bind(digest)
        .bind(cp.created_at)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "DELETE FROM app_checkpoints WHERE app_id = ? AND seq NOT IN
                 (SELECT seq FROM app_checkpoints WHERE app_id = ? ORDER BY seq DESC LIMIT ?)",
        )
        .bind(app_id)
        .bind(app_id)
        .bind(i64::try_from(MAX_APP_CHECKPOINTS).unwrap_or(60))
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(Some(cp))
    }

    /// An app's checkpoints, newest first.
    pub async fn app_checkpoints(&self, app_id: &str) -> Result<Vec<AppCheckpoint>> {
        let rows = sqlx::query(
            "SELECT id, app_id, label, kind, file_count, created_at FROM app_checkpoints
             WHERE app_id = ? ORDER BY seq DESC",
        )
        .bind(app_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|r| {
                Ok(AppCheckpoint {
                    id: r.try_get("id")?,
                    app_id: r.try_get("app_id")?,
                    label: r.try_get("label")?,
                    kind: CheckpointKind::parse(&r.try_get::<String, _>("kind")?),
                    files: u32::try_from(r.try_get::<i64, _>("file_count")?).unwrap_or(0),
                    created_at: r.try_get("created_at")?,
                })
            })
            .collect()
    }

    /// A checkpoint's app id and files (JSON).
    pub async fn app_checkpoint_files(&self, id: &str) -> Result<(String, String)> {
        let row: Option<(String, String)> =
            sqlx::query_as("SELECT app_id, files FROM app_checkpoints WHERE id = ?")
                .bind(id)
                .fetch_optional(&self.pool)
                .await?;
        row.ok_or(StoreError::NotFound { what: "checkpoint" })
    }

    /// Forgets an app's builder chat and checkpoints (the app or its draft was deleted).
    pub async fn delete_builder_data(&self, app_id: &str) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "DELETE FROM conversations WHERE id IN
                 (SELECT conversation_id FROM app_builder_chats WHERE app_id = ?)",
        )
        .bind(app_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM app_builder_chats WHERE app_id = ?")
            .bind(app_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM app_checkpoints WHERE app_id = ?")
            .bind(app_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use oxide_core::{CheckpointKind, StartConversation};

    use crate::test_util::store;

    #[tokio::test]
    async fn builder_chats_stay_out_of_the_chat_list_and_search() {
        let s = store().await;
        let (c, _) = s
            .start_conversation(StartConversation {
                text: "chart helper".into(),
                model: "p/m".into(),
                project_id: None,
                attachments: vec![],
                folders: vec![],
                tools: None,
            })
            .await
            .expect("chat");
        let b = s
            .create_builder_chat("my-app", "Building chart helper", "p/m")
            .await
            .expect("builder");
        assert_eq!(
            s.create_builder_chat("my-app", "x", "p/m")
                .await
                .expect("same"),
            b
        );
        assert_eq!(
            s.builder_app_of(&b).await.expect("ok").as_deref(),
            Some("my-app")
        );
        assert_eq!(s.builder_app_of(&c.id).await.expect("ok"), None);
        let list = s.list_conversations().await.expect("list");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, c.id);
        // Still reachable by id (the builder shows it).
        assert_eq!(s.get_conversation(&b).await.expect("get").id, b);
        let hits = s.search("chart", 10).await.expect("search");
        assert!(hits.iter().all(|h| h.conversation.id != b));
        s.delete_builder_data("my-app").await.expect("delete");
        assert!(s.get_conversation(&b).await.is_err());
        assert_eq!(s.builder_chat("my-app").await.expect("ok"), None);
    }

    #[tokio::test]
    async fn checkpoints_skip_repeats_keep_the_newest_and_return_their_files() {
        let s = store().await;
        let a = s
            .add_app_checkpoint("a", "Start", CheckpointKind::Start, 2, "[1]", "d1")
            .await
            .expect("add")
            .expect("made");
        assert!(
            s.add_app_checkpoint("a", "same", CheckpointKind::Model, 2, "[1]", "d1")
                .await
                .expect("add")
                .is_none()
        );
        s.add_app_checkpoint("b", "Other app", CheckpointKind::Start, 1, "[9]", "d1")
            .await
            .expect("add")
            .expect("made");
        let second = s
            .add_app_checkpoint("a", "After: blue", CheckpointKind::Model, 3, "[2]", "d2")
            .await
            .expect("add")
            .expect("made");
        let list = s.app_checkpoints("a").await.expect("list");
        assert_eq!(
            list.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            vec![second.id.as_str(), a.id.as_str()]
        );
        assert_eq!(list[0].kind, CheckpointKind::Model);
        assert_eq!(list[0].files, 3);
        assert_eq!(
            s.app_checkpoint_files(&a.id).await.expect("files"),
            ("a".to_owned(), "[1]".to_owned())
        );
        for i in 0..70 {
            s.add_app_checkpoint("a", "n", CheckpointKind::Manual, 1, "[]", &format!("x{i}"))
                .await
                .expect("add");
        }
        assert_eq!(
            s.app_checkpoints("a").await.expect("list").len(),
            oxide_core::MAX_APP_CHECKPOINTS
        );
        assert_eq!(s.app_checkpoints("b").await.expect("list").len(), 1);
        s.delete_builder_data("a").await.expect("delete");
        assert!(s.app_checkpoints("a").await.expect("list").is_empty());
    }
}
