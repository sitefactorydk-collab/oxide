//! Message repository. Messages form a tree per conversation (`parent_id`).

use oxide_core::message::plain_text;
use oxide_core::{ContentBlock, Feedback, Message, Role, StopReason, Usage};
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
    stop_reason: Option<String>,
    usage: Option<String>,
    feedback: Option<String>,
}

const MESSAGE_COLUMNS: &str = "id, conversation_id, parent_id, role, content, model, created_at, \
     stop_reason, usage, feedback";

/// A finished (or stopped) assistant reply to save.
#[derive(Debug, Clone, PartialEq)]
pub struct NewReply {
    /// Id chosen when the reply started, so the UI can key the live message by it.
    pub id: String,
    pub conversation_id: String,
    /// The user message being answered.
    pub parent_id: String,
    pub content: Vec<ContentBlock>,
    pub model: String,
    pub stop_reason: StopReason,
    pub usage: Option<Usage>,
}

impl TryFrom<MessageRow> for Message {
    type Error = StoreError;

    fn try_from(r: MessageRow) -> Result<Self> {
        let role = Role::parse(&r.role)
            .ok_or_else(|| StoreError::Corrupt(format!("unknown role in message {}", r.id)))?;
        let content: Vec<ContentBlock> = serde_json::from_str(&r.content)
            .map_err(|e| StoreError::Corrupt(format!("content of message {}: {e}", r.id)))?;
        // Usage is informational: unreadable usage is dropped rather than failing the load.
        let usage = r
            .usage
            .as_deref()
            .and_then(|u| serde_json::from_str::<Usage>(u).ok());
        let feedback = r
            .feedback
            .as_deref()
            .and_then(|f| serde_json::from_str::<Feedback>(f).ok());
        Ok(Message {
            id: r.id,
            conversation_id: r.conversation_id,
            parent_id: r.parent_id,
            role,
            content,
            model: r.model,
            created_at: r.created_at,
            stop_reason: r.stop_reason.as_deref().map(StopReason::parse),
            usage,
            feedback,
        })
    }
}

/// Inserts a message (no `updated_at` bump; callers do that).
pub(crate) async fn insert_message(conn: &mut SqliteConnection, m: &Message) -> Result<()> {
    let content = serde_json::to_string(&m.content)
        .map_err(|e| StoreError::Invalid(format!("content can't be serialized: {e}")))?;
    let usage = m
        .usage
        .map(|u| serde_json::to_string(&u))
        .transpose()
        .map_err(|e| StoreError::Invalid(format!("usage can't be serialized: {e}")))?;
    sqlx::query(
        "INSERT INTO messages
            (id, conversation_id, parent_id, role, content, search_text, model, created_at,
             stop_reason, usage)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&m.id)
    .bind(&m.conversation_id)
    .bind(&m.parent_id)
    .bind(m.role.as_str())
    .bind(content)
    .bind(plain_text(&m.content))
    .bind(&m.model)
    .bind(m.created_at)
    .bind(m.stop_reason.map(StopReason::as_str))
    .bind(usage)
    .execute(&mut *conn)
    .await
    .map_err(|e| map_fk(e, "conversation or parent message"))?;
    Ok(())
}

impl Store {
    /// Every message of a chat (all branches), in insertion order. A parent always comes
    /// before its children.
    pub async fn list_messages(&self, conversation_id: &str) -> Result<Vec<Message>> {
        let rows: Vec<MessageRow> = sqlx::query_as(&format!(
            "SELECT {MESSAGE_COLUMNS} FROM messages WHERE conversation_id = ? ORDER BY seq"
        ))
        .bind(conversation_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(Message::try_from).collect()
    }

    pub async fn get_message(&self, id: &str) -> Result<Message> {
        let row: Option<MessageRow> = sqlx::query_as(&format!(
            "SELECT {MESSAGE_COLUMNS} FROM messages WHERE id = ?"
        ))
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
        self.insert_child(Message {
            id: new_id(),
            conversation_id: conversation_id.to_owned(),
            parent_id: parent_id.map(str::to_owned),
            role,
            content,
            model,
            created_at: now_ms(),
            stop_reason: None,
            usage: None,
            feedback: None,
        })
        .await
    }

    /// Saves an assistant reply under the user message it answers.
    pub async fn add_reply(&self, reply: NewReply) -> Result<Message> {
        let parent = self.get_message(&reply.parent_id).await?;
        if parent.role != Role::User {
            return Err(StoreError::Invalid(
                "a reply must answer a user message".into(),
            ));
        }
        self.insert_child(Message {
            id: reply.id,
            conversation_id: reply.conversation_id,
            parent_id: Some(reply.parent_id),
            role: Role::Assistant,
            content: reply.content,
            model: Some(reply.model),
            created_at: now_ms(),
            stop_reason: Some(reply.stop_reason),
            usage: reply.usage,
            feedback: None,
        })
        .await
    }

    /// Inserts `message` after checking its parent is in the same chat, and bumps the chat.
    async fn insert_child(&self, message: Message) -> Result<Message> {
        let mut tx = self.pool.begin().await?;
        if let Some(parent) = message.parent_id.as_deref() {
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
                Some(c) if c != message.conversation_id => {
                    return Err(StoreError::Invalid(
                        "parent message belongs to another chat".into(),
                    ));
                }
                Some(_) => {}
            }
        }
        insert_message(&mut tx, &message).await?;
        sqlx::query("UPDATE conversations SET updated_at = ? WHERE id = ?")
            .bind(message.created_at)
            .bind(&message.conversation_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(message)
    }

    /// Edits a user message the way chat apps do: adds a sibling with the new text under the
    /// same parent, leaving the original branch in place. Attachments of the original are
    /// kept (before the text, as when it was sent).
    pub async fn edit_user_message(&self, message_id: &str, text: &str) -> Result<Message> {
        let text = text.trim();
        let original = self.get_message(message_id).await?;
        if original.role != Role::User {
            return Err(StoreError::Invalid(
                "only your own messages can be edited".into(),
            ));
        }
        let mut content: Vec<ContentBlock> = original
            .content
            .into_iter()
            .filter(|b| {
                matches!(
                    b,
                    ContentBlock::Image { .. } | ContentBlock::Document { .. }
                )
            })
            .collect();
        if !text.is_empty() {
            content.push(ContentBlock::text(text));
        }
        if content.is_empty() {
            return Err(StoreError::Invalid("message is empty".into()));
        }
        self.add_message(
            &original.conversation_id,
            original.parent_id.as_deref(),
            Role::User,
            content,
            None,
        )
        .await
    }

    /// Sets or clears the user's rating of an assistant reply.
    pub async fn set_feedback(
        &self,
        message_id: &str,
        feedback: Option<Feedback>,
    ) -> Result<Message> {
        let mut message = self.get_message(message_id).await?;
        if message.role != Role::Assistant {
            return Err(StoreError::Invalid("only replies can be rated".into()));
        }
        let feedback = feedback.map(|mut f| {
            f.note = f
                .note
                .map(|n| n.trim().chars().take(2_000).collect::<String>())
                .filter(|n| !n.is_empty());
            f
        });
        let json = feedback
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|e| StoreError::Invalid(format!("feedback can't be serialized: {e}")))?;
        sqlx::query("UPDATE messages SET feedback = ? WHERE id = ?")
            .bind(json)
            .bind(message_id)
            .execute(&self.pool)
            .await?;
        message.feedback = feedback;
        Ok(message)
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
                attachments: Vec::new(),
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
    async fn replies_keep_their_id_stop_reason_and_usage() {
        let s = store().await;
        let (cid, root) = chat(&s).await;
        let usage = Usage {
            input_tokens: 12,
            output_tokens: 34,
            ..Usage::default()
        };
        let reply = s
            .add_reply(NewReply {
                id: "reply-1".into(),
                conversation_id: cid.clone(),
                parent_id: root.id.clone(),
                content: vec![ContentBlock::text("partial")],
                model: "anthropic/x".into(),
                stop_reason: StopReason::Stopped,
                usage: Some(usage),
            })
            .await
            .expect("saves");
        let loaded = s.get_message("reply-1").await.expect("loads");
        assert_eq!(loaded, reply);
        assert_eq!(loaded.stop_reason, Some(StopReason::Stopped));
        assert_eq!(loaded.usage, Some(usage));
        assert_eq!(loaded.role, Role::Assistant);
        // A reply can't answer another reply.
        let err = s
            .add_reply(NewReply {
                id: "reply-2".into(),
                conversation_id: cid,
                parent_id: "reply-1".into(),
                content: vec![],
                model: "anthropic/x".into(),
                stop_reason: StopReason::EndTurn,
                usage: None,
            })
            .await;
        assert!(matches!(err, Err(StoreError::Invalid(_))));
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

    #[tokio::test]
    async fn edits_keep_attachments_and_feedback_round_trips() {
        let s = store().await;
        let image = ContentBlock::Image {
            media_type: "image/png".into(),
            source: oxide_core::MediaSource::Path {
                path: "/a/b.png".into(),
            },
        };
        let (c, root) = s
            .start_conversation(StartConversation {
                text: String::new(),
                model: "m".into(),
                project_id: None,
                attachments: vec![oxide_core::Attachment {
                    id: "a".into(),
                    name: "shot.png".into(),
                    kind: oxide_core::AttachmentKind::Image,
                    media_type: "image/png".into(),
                    path: "/a/b.png".into(),
                    size: 1,
                    original_size: 1,
                    width: None,
                    height: None,
                    pages: None,
                    thumbnail: None,
                    note: None,
                }],
            })
            .await
            .expect("starts with only a file");
        assert_eq!(c.title, "shot.png");
        assert_eq!(root.content, vec![image.clone()]);

        let edited = s
            .edit_user_message(&root.id, "what is this?")
            .await
            .expect("edits");
        assert_eq!(
            edited.content,
            vec![image.clone(), ContentBlock::text("what is this?")]
        );
        // Text can be removed as long as a file is left.
        let bare = s.edit_user_message(&edited.id, "  ").await.expect("edits");
        assert_eq!(bare.content, vec![image]);

        let reply = s
            .add_message(&c.id, Some(&root.id), Role::Assistant, vec![], None)
            .await
            .expect("adds");
        let rated = s
            .set_feedback(
                &reply.id,
                Some(Feedback {
                    rating: oxide_core::Rating::Bad,
                    note: Some("  wrong year  ".into()),
                    created_at: 5,
                }),
            )
            .await
            .expect("rates");
        assert_eq!(
            rated.feedback.as_ref().and_then(|f| f.note.as_deref()),
            Some("wrong year")
        );
        assert_eq!(s.get_message(&reply.id).await.expect("ok"), rated);
        let cleared = s.set_feedback(&reply.id, None).await.expect("clears");
        assert_eq!(cleared.feedback, None);
        assert_eq!(s.get_message(&reply.id).await.expect("ok").feedback, None);
        assert!(matches!(
            s.set_feedback(&root.id, None).await,
            Err(StoreError::Invalid(_))
        ));
    }
}
