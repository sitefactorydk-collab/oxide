//! Stored conversation summaries (Phase 13b).

use oxide_core::{Compaction, CompactionTrigger};

use crate::{Result, Store};

type Row = (
    String,
    String,
    String,
    Option<String>,
    String,
    String,
    i64,
    i64,
    Option<String>,
    i64,
);

fn from_row(r: Row) -> Compaction {
    let (
        id,
        conversation_id,
        message_id,
        after_tool_use_id,
        summary,
        trigger,
        tokens_before,
        tokens_after,
        model,
        created_at,
    ) = r;
    Compaction {
        id,
        conversation_id,
        message_id,
        after_tool_use_id,
        summary,
        trigger: CompactionTrigger::parse(&trigger),
        tokens_before: u32::try_from(tokens_before).unwrap_or(0),
        tokens_after: u32::try_from(tokens_after).unwrap_or(0),
        model,
        created_at,
    }
}

impl Store {
    /// Saves a compaction (its id and time are the caller's).
    pub async fn add_compaction(&self, c: &Compaction) -> Result<()> {
        sqlx::query(
            "INSERT INTO compactions (id, conversation_id, message_id, after_tool_use_id, summary, \
             trigger, tokens_before, tokens_after, model, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&c.id)
        .bind(&c.conversation_id)
        .bind(&c.message_id)
        .bind(&c.after_tool_use_id)
        .bind(&c.summary)
        .bind(c.trigger.as_str())
        .bind(i64::from(c.tokens_before))
        .bind(i64::from(c.tokens_after))
        .bind(&c.model)
        .bind(c.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// A chat's compactions, oldest first.
    pub async fn compactions(&self, conversation_id: &str) -> Result<Vec<Compaction>> {
        let rows: Vec<Row> = sqlx::query_as(
            "SELECT id, conversation_id, message_id, after_tool_use_id, summary, trigger, \
             tokens_before, tokens_after, model, created_at FROM compactions \
             WHERE conversation_id = ? ORDER BY created_at, rowid",
        )
        .bind(conversation_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(from_row).collect())
    }
}

#[cfg(test)]
mod tests {
    use crate::test_util::store;
    use oxide_core::{Compaction, CompactionTrigger, StartConversation};

    #[tokio::test]
    async fn compactions_are_saved_listed_and_go_with_the_chat() {
        let s = store().await;
        let (c, first) = s
            .start_conversation(StartConversation {
                text: "Hi".into(),
                model: "a/b".into(),
                project_id: None,
                attachments: Vec::new(),
                folders: vec![],
                tools: None,
            })
            .await
            .expect("chat");
        let mk = |id: &str, at: i64, anchor: Option<&str>| Compaction {
            id: id.into(),
            conversation_id: c.id.clone(),
            message_id: first.id.clone(),
            after_tool_use_id: anchor.map(str::to_owned),
            summary: format!("summary {id}"),
            trigger: CompactionTrigger::Overflow,
            tokens_before: 1000,
            tokens_after: 100,
            model: Some("a/b".into()),
            created_at: at,
        };
        s.add_compaction(&mk("2", 20, Some("t1")))
            .await
            .expect("adds");
        s.add_compaction(&mk("1", 10, None)).await.expect("adds");
        let all = s.compactions(&c.id).await.expect("lists");
        assert_eq!(all.len(), 2);
        assert_eq!(all[0], mk("1", 10, None));
        assert_eq!(all[1].after_tool_use_id.as_deref(), Some("t1"));
        assert_eq!(all[1].trigger, CompactionTrigger::Overflow);
        s.delete_conversation(&c.id).await.expect("deletes");
        assert!(s.compactions(&c.id).await.expect("lists").is_empty());
    }
}
