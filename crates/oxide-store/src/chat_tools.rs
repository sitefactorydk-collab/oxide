//! Per-chat tool switches (which MCP servers and tools a chat may use).

use oxide_core::ChatTools;

use crate::{Result, Store, StoreError};

impl Store {
    /// The chat's switches; everything on if none were saved.
    pub async fn chat_tools(&self, conversation_id: &str) -> Result<ChatTools> {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT settings FROM chat_tools WHERE conversation_id = ?")
                .bind(conversation_id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row
            .and_then(|(json,)| serde_json::from_str(&json).ok())
            .unwrap_or_default())
    }

    pub async fn set_chat_tools(&self, conversation_id: &str, tools: &ChatTools) -> Result<()> {
        // Fails with NotFound for unknown chats rather than a foreign key error.
        self.get_conversation(conversation_id).await?;
        let json = serde_json::to_string(tools).map_err(|e| StoreError::Corrupt(e.to_string()))?;
        sqlx::query(
            "INSERT INTO chat_tools (conversation_id, settings) VALUES (?, ?)
             ON CONFLICT(conversation_id) DO UPDATE SET settings = excluded.settings",
        )
        .bind(conversation_id)
        .bind(json)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use oxide_core::{ChatTools, StartConversation};

    use crate::Store;

    #[tokio::test]
    async fn chat_tools_round_trip_and_cascade() {
        let store = Store::open_in_memory().await.expect("store");
        let (chat, _) = store
            .start_conversation(StartConversation {
                text: "hi".into(),
                model: "p/m".into(),
                project_id: None,
                attachments: vec![],
            })
            .await
            .expect("chat");
        assert_eq!(
            store.chat_tools(&chat.id).await.expect("tools"),
            ChatTools::default()
        );
        let t = ChatTools {
            disabled_servers: vec!["fs".into()],
            disabled_tools: vec!["web/fetch".into()],
            disabled_builtins: vec!["web_search".into()],
        };
        store.set_chat_tools(&chat.id, &t).await.expect("saves");
        store
            .set_chat_tools(&chat.id, &t)
            .await
            .expect("saves again");
        assert_eq!(store.chat_tools(&chat.id).await.expect("tools"), t);
        assert!(store.set_chat_tools("nope", &t).await.is_err());
        store.delete_conversation(&chat.id).await.expect("deletes");
        assert_eq!(
            store.chat_tools(&chat.id).await.expect("tools"),
            ChatTools::default()
        );
    }
}
