//! Message repository. Messages form a tree per conversation (`parent_id`).

use oxide_core::message::plain_text;
use oxide_core::{ContentBlock, Message, Role};
use sqlx::SqliteConnection;

use crate::conversations::map_fk;
use crate::{Result, Store, StoreError, new_id, now_ms};

#[derive(sqlx::FromRow)]
struct MessageRow {
    id: String,
    conversation_id: String,
    parent_id: Option<String>,
    role: String,
    content: String,
    model: Option<String>,
    created_at: i64,
}

impl TryFrom<MessageRow> for Message {
    type Error = StoreError;

    fn try_from(r: MessageRow) -> Result<Self> {
        let role = Role::parse(&r.role)
            .ok_or_else(|| StoreError::Corrupt(format!("unknown role in message {}", r.id)))?;
        let content: Vec<ContentBlock> = serde_json::from_str(&r.content)
            .map_err(|e| StoreError::Corrupt(format!("content of message {}: {e}", r.id)))?;
        Ok(Message {
            id: r.id,
            conversation_id: r.conversation_id,
            parent_id: r.parent_id,
            role,
            content,
            model: r.model,
            created_at: r.created_at,
        })
    }
}

/// Inserts a message (no `updated_at` bump; callers do that).
pub(crate) async fn insert_message(conn: &mut SqliteConnection, m: &Message) -> Result<()> {
    let content = serde_json::to_string(&m.content)
        .map_err(|e| StoreError::Invalid(format!("content can't be serialized: {e}")))?;
    sqlx::query(
        "INSERT INTO messages (id, conversation_id, parent_id, role, content, search_text, model, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&m.id)
    .bind(&m.conversation_id)
    .bind(&m.parent_id)
    .bind(m.role.as_str())
    .bind(content)
    .bind(plain_text(&m.content))
    .bind(&m.model)
    .bind(m.created_at)
    .execute(&mut *conn)
    .await
    .map_err(|e| map_fk(e, "conversation or parent message"))?;
    Ok(())
}

impl Store {
    /// Every message of a chat (all branches), in insertion order. A parent always comes
    /// before its children.
    pub async fn list_messages(&self, conversation_id: &str) -> Result<Vec<Message>> {
        let rows: Vec<MessageRow> = sqlx::query_as(
            "SELECT id, conversation_id, parent_id, role, content, model, created_at
             FROM messages WHERE conversation_id = ? ORDER BY seq",
        )
        .bind(conversation_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(Message::try_from).collect()
    }

    pub async fn get_message(&self, id: &str) -> Result<Message> {
        let row: Option<MessageRow> = sqlx::query_as(
            "SELECT id, conversation_id, parent_id, role, content, model, created_at
             FROM messages WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        row.ok_or(StoreError::NotFound { what: "message" })?
            .try_into()
    }

    /// Appends a message under `parent_id` and bumps the chat's `updated_at`.
    ///
    /// The parent must belong to the same chat.
    pub async fn add_message(
        &self,
        conversation_id: &str,
        parent_id: Option<&str>,
        role: Role,
        content: Vec<ContentBlock>,
        model: Option<String>,
    ) -> Result<Message> {
        let mut tx = self.pool.begin().await?;
        if let Some(parent) = parent_id {
            let owner: Option<String> =
                sqlx::query_scalar("SELECT conversation_id FROM messages WHERE id = ?")
                    .bind(parent)
                    .fetch_optional(&mut *tx)
                    .await?;
            match owner {
                None => {
                    return Err(StoreError::NotFound {
                        what: "parent message",
                    });
                }
                Some(c) if c != conversation_id => {
                    return Err(StoreError::Invalid(
                        "parent message belongs to another chat".into(),
                    ));
                }
                Some(_) => {}
            }
        }
        let message = Message {
            id: new_id(),
            conversation_id: conversation_id.to_owned(),
            parent_id: parent_id.map(str::to_owned),
            role,
            content,
            model,
            created_at: now_ms(),
        };
        insert_message(&mut tx, &message).await?;
        sqlx::query("UPDATE conversations SET updated_at = ? WHERE id = ?")
            .bind(message.created_at)
            .bind(conversation_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(message)
    }

    /// Edits a user message the way chat apps do: adds a sibling with the new text under the
    /// same parent, leaving the original branch in place.
    pub async fn edit_user_message(&self, message_id: &str, text: &str) -> Result<Message> {
        let text = text.trim();
        if text.is_empty() {
            return Err(StoreError::Invalid("message is empty".into()));
        }
        let original = self.get_message(message_id).await?;
        if original.role != Role::User {
            return Err(StoreError::Invalid(
                "only your own messages can be edited".into(),
            ));
        }
        self.add_message(
            &original.conversation_id,
            original.parent_id.as_deref(),
            Role::User,
            vec![ContentBlock::text(text)],
            None,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::store;
    use oxide_core::StartConversation;

    async fn chat(s: &Store) -> (String, Message) {
        let (c, m) = s
            .start_conversation(StartConversation {
                text: "root".into(),
                model: "m".into(),
                project_id: None,
            })
            .await
            .expect("ok");
        (c.id, m)
    }

    #[tokio::test]
    async fn add_message_links_parent_and_bumps_chat() {
        let s = store().await;
        let (cid, root) = chat(&s).await;
        let before = s.get_conversation(&cid).await.expect("ok").updated_at;
        let reply = s
            .add_message(
                &cid,
                Some(&root.id),
                Role::Assistant,
                vec![ContentBlock::text("hi")],
                Some("m".into()),
            )
            .await
            .expect("adds");
        assert_eq!(reply.parent_id.as_deref(), Some(root.id.as_str()));
        assert!(s.get_conversation(&cid).await.expect("ok").updated_at >= before);
        let ids: Vec<_> = s
            .list_messages(&cid)
            .await
            .expect("ok")
            .into_iter()
            .map(|m| m.id)
            .collect();
        assert_eq!(ids, vec![root.id, reply.id]);
    }

    #[tokio::test]
    async fn parent_must_exist_in_the_same_chat() {
        let s = store().await;
        let (a, _) = chat(&s).await;
        let (_, other_root) = chat(&s).await;
        let err = s
            .add_message(&a, Some(&other_root.id), Role::User, vec![], None)
            .await;
        assert!(matches!(err, Err(StoreError::Invalid(_))));
        let err = s
            .add_message(&a, Some("missing"), Role::User, vec![], None)
            .await;
        assert!(matches!(err, Err(StoreError::NotFound { .. })));
    }

    #[tokio::test]
    async fn edit_adds_a_sibling() {
        let s = store().await;
        let (cid, root) = chat(&s).await;
        let edited = s
            .edit_user_message(&root.id, "root v2")
            .await
            .expect("edits");
        assert_eq!(edited.parent_id, None);
        assert_eq!(s.list_messages(&cid).await.expect("ok").len(), 2);
        assert_eq!(
            s.get_message(&root.id).await.expect("ok").content,
            vec![ContentBlock::text("root")]
        );
    }

    #[tokio::test]
    async fn assistant_messages_cannot_be_edited() {
        let s = store().await;
        let (cid, root) = chat(&s).await;
        let reply = s
            .add_message(&cid, Some(&root.id), Role::Assistant, vec![], None)
            .await
            .expect("adds");
        assert!(matches!(
            s.edit_user_message(&reply.id, "x").await,
            Err(StoreError::Invalid(_))
        ));
    }
}
