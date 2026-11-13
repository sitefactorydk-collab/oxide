//! Tool approvals saved per chat, and the log of tool calls each chat made.

use oxide_core::{ToolUsage, ToolUsageKind};

use crate::{Result, Store, new_id, now_ms};

fn kind_str(kind: ToolUsageKind) -> &'static str {
    match kind {
        ToolUsageKind::WebSearch => "webSearch",
        ToolUsageKind::WebFetch => "webFetch",
        ToolUsageKind::Connector => "connector",
        ToolUsageKind::ProviderTool => "providerTool",
        ToolUsageKind::ProjectSearch => "projectSearch",
        ToolUsageKind::Artifact => "artifact",
        ToolUsageKind::Skill => "skill",
        ToolUsageKind::File => "file",
        ToolUsageKind::Shell => "shell",
        ToolUsageKind::Tasks => "tasks",
        ToolUsageKind::Python => "python",
        ToolUsageKind::Sandbox => "sandbox",
        ToolUsageKind::Browser => "browser",
        ToolUsageKind::Chrome => "chrome",
        ToolUsageKind::Computer => "computer",
    }
}

fn parse_kind(s: &str) -> ToolUsageKind {
    match s {
        "webSearch" => ToolUsageKind::WebSearch,
        "webFetch" => ToolUsageKind::WebFetch,
        "providerTool" => ToolUsageKind::ProviderTool,
        "projectSearch" => ToolUsageKind::ProjectSearch,
        "artifact" => ToolUsageKind::Artifact,
        "skill" => ToolUsageKind::Skill,
        "file" => ToolUsageKind::File,
        "shell" => ToolUsageKind::Shell,
        "tasks" => ToolUsageKind::Tasks,
        "python" => ToolUsageKind::Python,
        "sandbox" => ToolUsageKind::Sandbox,
        "browser" => ToolUsageKind::Browser,
        "chrome" => ToolUsageKind::Chrome,
        "computer" => ToolUsageKind::Computer,
        _ => ToolUsageKind::Connector,
    }
}

/// A tool call to log (see [`Store::log_tool_use`]).
#[derive(Debug, Clone)]
pub struct NewToolUsage {
    pub conversation_id: String,
    pub message_id: String,
    pub tool_use_id: String,
    pub kind: ToolUsageKind,
    pub server: Option<String>,
    pub tool: String,
    pub label: Option<String>,
    pub is_error: bool,
}

type UsageRow = (
    String,
    String,
    String,
    String,
    String,
    Option<String>,
    String,
    Option<String>,
    bool,
    i64,
);

impl Store {
    /// Tools the user allowed for the rest of a chat (`server/tool` or `builtin/<name>`).
    pub async fn chat_tool_grants(&self, conversation_id: &str) -> Result<Vec<String>> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT tool_key FROM chat_tool_grants WHERE conversation_id = ? ORDER BY created_at",
        )
        .bind(conversation_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|(k,)| k).collect())
    }

    pub async fn grant_chat_tool(&self, conversation_id: &str, tool_key: &str) -> Result<()> {
        self.get_conversation(conversation_id).await?;
        sqlx::query(
            "INSERT INTO chat_tool_grants (conversation_id, tool_key, created_at) VALUES (?, ?, ?)
             ON CONFLICT DO NOTHING",
        )
        .bind(conversation_id)
        .bind(tool_key)
        .bind(now_ms())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Withdraws "Allow always" for one tool, or for all tools of a chat (`None`).
    pub async fn revoke_chat_tools(
        &self,
        conversation_id: &str,
        tool_key: Option<&str>,
    ) -> Result<()> {
        match tool_key {
            Some(k) => {
                sqlx::query(
                    "DELETE FROM chat_tool_grants WHERE conversation_id = ? AND tool_key = ?",
                )
                .bind(conversation_id)
                .bind(k)
                .execute(&self.pool)
                .await?
            }
            None => {
                sqlx::query("DELETE FROM chat_tool_grants WHERE conversation_id = ?")
                    .bind(conversation_id)
                    .execute(&self.pool)
                    .await?
            }
        };
        Ok(())
    }

    /// Records a finished tool call.
    pub async fn log_tool_use(&self, u: NewToolUsage) -> Result<ToolUsage> {
        let usage = ToolUsage {
            id: new_id(),
            conversation_id: u.conversation_id,
            message_id: u.message_id,
            tool_use_id: u.tool_use_id,
            kind: u.kind,
            server: u.server,
            tool: u.tool,
            label: u.label.map(|l| l.chars().take(300).collect()),
            is_error: u.is_error,
            created_at: now_ms(),
        };
        sqlx::query(
            "INSERT INTO tool_usage (id, conversation_id, message_id, tool_use_id, kind, server,
                                     tool, label, is_error, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&usage.id)
        .bind(&usage.conversation_id)
        .bind(&usage.message_id)
        .bind(&usage.tool_use_id)
        .bind(kind_str(usage.kind))
        .bind(&usage.server)
        .bind(&usage.tool)
        .bind(&usage.label)
        .bind(usage.is_error)
        .bind(usage.created_at)
        .execute(&self.pool)
        .await?;
        Ok(usage)
    }

    /// Every tool call of a chat, oldest first.
    pub async fn tool_usage(&self, conversation_id: &str) -> Result<Vec<ToolUsage>> {
        let rows: Vec<UsageRow> = sqlx::query_as(
            "SELECT id, conversation_id, message_id, tool_use_id, kind, server, tool, label,
                    is_error, created_at
             FROM tool_usage WHERE conversation_id = ? ORDER BY created_at, rowid",
        )
        .bind(conversation_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(
                |(
                    id,
                    conversation_id,
                    message_id,
                    tool_use_id,
                    kind,
                    server,
                    tool,
                    label,
                    is_error,
                    created_at,
                )| {
                    ToolUsage {
                        id,
                        conversation_id,
                        message_id,
                        tool_use_id,
                        kind: parse_kind(&kind),
                        server,
                        tool,
                        label,
                        is_error,
                        created_at,
                    }
                },
            )
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use oxide_core::{StartConversation, ToolUsageKind};

    use super::NewToolUsage;
    use crate::Store;

    #[tokio::test]
    async fn grants_and_usage_round_trip_and_cascade() {
        let store = Store::open_in_memory().await.expect("store");
        let (chat, _) = store
            .start_conversation(StartConversation {
                text: "hi".into(),
                model: "p/m".into(),
                project_id: None,
                attachments: vec![],
                folders: vec![],
                tools: None,
            })
            .await
            .expect("chat");
        store
            .grant_chat_tool(&chat.id, "fs/read")
            .await
            .expect("grant");
        store
            .grant_chat_tool(&chat.id, "fs/read")
            .await
            .expect("again");
        store
            .grant_chat_tool(&chat.id, "builtin/web_fetch")
            .await
            .expect("grant");
        assert_eq!(
            store
                .chat_tool_grants(&chat.id)
                .await
                .expect("grants")
                .len(),
            2
        );
        store
            .revoke_chat_tools(&chat.id, Some("fs/read"))
            .await
            .expect("revoke");
        assert_eq!(
            store.chat_tool_grants(&chat.id).await.expect("grants"),
            vec!["builtin/web_fetch".to_owned()]
        );
        assert!(store.grant_chat_tool("nope", "x/y").await.is_err());

        for (kind, label) in [
            (ToolUsageKind::WebSearch, "rust tauri"),
            (ToolUsageKind::Connector, "C:/x"),
        ] {
            store
                .log_tool_use(NewToolUsage {
                    conversation_id: chat.id.clone(),
                    message_id: "m".into(),
                    tool_use_id: "t".into(),
                    kind,
                    server: None,
                    tool: "web_search".into(),
                    label: Some(label.into()),
                    is_error: false,
                })
                .await
                .expect("logs");
        }
        let used = store.tool_usage(&chat.id).await.expect("usage");
        assert_eq!(used.len(), 2);
        assert_eq!(used[0].kind, ToolUsageKind::WebSearch);
        assert_eq!(used[1].label.as_deref(), Some("C:/x"));

        store.delete_conversation(&chat.id).await.expect("deletes");
        assert!(store.tool_usage(&chat.id).await.expect("usage").is_empty());
        assert!(
            store
                .chat_tool_grants(&chat.id)
                .await
                .expect("grants")
                .is_empty()
        );
    }
}
