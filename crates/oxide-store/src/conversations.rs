//! Conversation repository.

use oxide_core::conversation::{clean_title, title_from_text};
use oxide_core::{ContentBlock, Conversation, Message, Role, StartConversation};

use crate::messages::insert_message;
use crate::{Result, Store, StoreError, new_id, now_ms};

#[derive(sqlx::FromRow)]
pub(crate) struct ConversationRow {
    id: String,
    title: String,
    project_id: Option<String>,
    model: String,
    starred: bool,
    created_at: i64,
    updated_at: i64,
}

impl From<ConversationRow> for Conversation {
    fn from(r: ConversationRow) -> Self {
        Conversation {
            id: r.id,
            title: r.title,
            project_id: r.project_id,
            model: r.model,
            starred: r.starred,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

pub(crate) const CONVERSATION_COLUMNS: &str =
    "id, title, project_id, model, starred, created_at, updated_at";

const NOT_FOUND: StoreError = StoreError::NotFound {
    what: "conversation",
};

impl Store {
    /// All chats, most recently updated first.
    pub async fn list_conversations(&self) -> Result<Vec<Conversation>> {
        let rows: Vec<ConversationRow> = sqlx::query_as(&format!(
            "SELECT {CONVERSATION_COLUMNS} FROM conversations ORDER BY updated_at DESC, seq DESC"
        ))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn get_conversation(&self, id: &str) -> Result<Conversation> {
        let row: Option<ConversationRow> = sqlx::query_as(&format!(
            "SELECT {CONVERSATION_COLUMNS} FROM conversations WHERE id = ?"
        ))
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(Into::into).ok_or(NOT_FOUND)
    }

    /// Creates a chat titled after its first message, and that message, in one transaction.
    pub async fn start_conversation(
        &self,
        input: StartConversation,
    ) -> Result<(Conversation, Message)> {
        let model = input.model.trim();
        if model.is_empty() {
            return Err(StoreError::Invalid("a model is required".into()));
        }
        let text = input.text.trim();
        let mut content: Vec<ContentBlock> =
            input.attachments.iter().map(|a| a.to_block()).collect();
        if !text.is_empty() {
            content.push(ContentBlock::text(text));
        }
        if content.is_empty() {
            return Err(StoreError::Invalid("the message is empty".into()));
        }
        // A message with only files is titled after the first file.
        let title_source = if text.is_empty() {
            input.attachments.first().map_or("", |a| a.name.as_str())
        } else {
            text
        };
        let now = now_ms();
        let convo = Conversation {
            id: new_id(),
            title: title_from_text(title_source),
            project_id: input.project_id.filter(|p| !p.is_empty()),
            model: model.to_owned(),
            starred: false,
            created_at: now,
            updated_at: now,
        };
        let message = Message {
            id: new_id(),
            conversation_id: convo.id.clone(),
            parent_id: None,
            role: Role::User,
            content,
            model: None,
            created_at: now,
            stop_reason: None,
            usage: None,
            feedback: None,
        };

        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO conversations (id, title, project_id, model, starred, created_at, updated_at)
             VALUES (?, ?, ?, ?, 0, ?, ?)",
        )
        .bind(&convo.id)
        .bind(&convo.title)
        .bind(&convo.project_id)
        .bind(&convo.model)
        .bind(convo.created_at)
        .bind(convo.updated_at)
        .execute(&mut *tx)
        .await
        .map_err(|e| map_fk(e, "project"))?;
        insert_message(&mut tx, &message).await?;
        for folder in &input.folders {
            crate::workspace::insert_folder(&mut tx, &convo.id, folder).await?;
        }
        if let Some(tools) = &input.tools {
            let json =
                serde_json::to_string(tools).map_err(|e| StoreError::Corrupt(e.to_string()))?;
            sqlx::query("INSERT INTO chat_tools (conversation_id, settings) VALUES (?, ?)")
                .bind(&convo.id)
                .bind(json)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok((convo, message))
    }

    pub async fn rename_conversation(&self, id: &str, title: &str) -> Result<Conversation> {
        let title =
            clean_title(title).ok_or_else(|| StoreError::Invalid("title is empty".into()))?;
        self.update_conversation(
            id,
            sqlx::query("UPDATE conversations SET title = ?, updated_at = ? WHERE id = ?")
                .bind(title)
                .bind(now_ms()),
        )
        .await
    }

    /// Stars or unstars a chat. Doesn't bump `updated_at`, so the chat keeps its place in Recents.
    pub async fn set_starred(&self, id: &str, starred: bool) -> Result<Conversation> {
        self.update_conversation(
            id,
            sqlx::query("UPDATE conversations SET starred = ? WHERE id = ?").bind(starred),
        )
        .await
    }

    pub async fn set_conversation_model(&self, id: &str, model: &str) -> Result<Conversation> {
        let model = model.trim();
        if model.is_empty() {
            return Err(StoreError::Invalid("a model is required".into()));
        }
        self.update_conversation(
            id,
            sqlx::query("UPDATE conversations SET model = ? WHERE id = ?").bind(model),
        )
        .await
    }

    /// Moves a chat into a project (`Some`) or out of any project (`None`). Doesn't bump
    /// `updated_at`, so the chat keeps its place in Recents.
    pub async fn set_conversation_project(
        &self,
        id: &str,
        project_id: Option<&str>,
    ) -> Result<Conversation> {
        let project_id = project_id.filter(|p| !p.is_empty());
        let done = sqlx::query("UPDATE conversations SET project_id = ? WHERE id = ?")
            .bind(project_id)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| map_fk(e, "project"))?;
        if done.rows_affected() == 0 {
            return Err(NOT_FOUND);
        }
        self.get_conversation(id).await
    }

    /// Deletes a chat and (by cascade) its messages and their search index entries.
    /// Deletes a chat with everything in it. A scheduled task's run that wrote the chat
    /// leaves the task's history with it.
    pub async fn delete_conversation(&self, id: &str) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM task_runs WHERE conversation_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        let done = sqlx::query("DELETE FROM conversations WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        if done.rows_affected() == 0 {
            return Err(NOT_FOUND);
        }
        tx.commit().await?;
        Ok(())
    }

    /// Runs an UPDATE whose last placeholder is the conversation id, then returns the row.
    async fn update_conversation<'q>(
        &self,
        id: &'q str,
        query: sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'q>>,
    ) -> Result<Conversation> {
        let done = query.bind(id).execute(&self.pool).await?;
        if done.rows_affected() == 0 {
            return Err(NOT_FOUND);
        }
        self.get_conversation(id).await
    }
}

/// Turns a foreign-key violation into a readable "not found".
pub(crate) fn map_fk(err: sqlx::Error, what: &'static str) -> StoreError {
    if let sqlx::Error::Database(db) = &err
        && db.is_foreign_key_violation()
    {
        return StoreError::NotFound { what };
    }
    StoreError::Db(err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::store;

    fn start(text: &str) -> StartConversation {
        StartConversation {
            text: text.into(),
            model: "anthropic/sonnet".into(),
            project_id: None,
            attachments: Vec::new(),
            folders: vec![],
            tools: None,
        }
    }

    #[tokio::test]
    async fn start_creates_chat_and_first_message() {
        let s = store().await;
        let (c, m) = s
            .start_conversation(start("Plan the release\nmore"))
            .await
            .expect("ok");
        assert_eq!(c.title, "Plan the release");
        assert_eq!(m.conversation_id, c.id);
        assert_eq!(m.parent_id, None);
        assert_eq!(s.list_messages(&c.id).await.expect("lists"), vec![m]);
    }

    #[tokio::test]
    async fn list_is_newest_first() {
        let s = store().await;
        let (a, _) = s.start_conversation(start("first")).await.expect("ok");
        let (b, _) = s.start_conversation(start("second")).await.expect("ok");
        let ids: Vec<String> = s
            .list_conversations()
            .await
            .expect("lists")
            .into_iter()
            .map(|c| c.id)
            .collect();
        assert_eq!(ids, vec![b.id, a.id]);
    }

    #[tokio::test]
    async fn rename_star_and_model() {
        let s = store().await;
        let (c, _) = s.start_conversation(start("x")).await.expect("ok");
        assert_eq!(
            s.rename_conversation(&c.id, "  New  ")
                .await
                .expect("ok")
                .title,
            "New"
        );
        assert!(matches!(
            s.rename_conversation(&c.id, " ").await,
            Err(StoreError::Invalid(_))
        ));
        let starred = s.set_starred(&c.id, true).await.expect("ok");
        assert!(starred.starred);
        let m = s
            .set_conversation_model(&c.id, "openai/gpt")
            .await
            .expect("ok");
        assert_eq!(m.model, "openai/gpt");
    }

    #[tokio::test]
    async fn delete_cascades_to_messages() {
        let s = store().await;
        let (c, _) = s.start_conversation(start("bye")).await.expect("ok");
        s.delete_conversation(&c.id).await.expect("deletes");
        assert!(matches!(
            s.get_conversation(&c.id).await,
            Err(StoreError::NotFound { .. })
        ));
        assert!(s.list_messages(&c.id).await.expect("lists").is_empty());
        assert!(matches!(
            s.delete_conversation(&c.id).await,
            Err(StoreError::NotFound { .. })
        ));
    }

    #[tokio::test]
    async fn unknown_project_is_rejected() {
        let s = store().await;
        let mut input = start("x");
        input.project_id = Some("nope".into());
        assert!(matches!(
            s.start_conversation(input).await,
            Err(StoreError::NotFound { what: "project" })
        ));
    }
}
