//! Memory items (opt-in): facts about the user kept across chats.

use oxide_core::memory::clean_memory_text;
use oxide_core::{MemoryItem, NewMemory};

use crate::conversations::map_fk;
use crate::{Result, Store, StoreError, new_id, now_ms};

/// Most memory items Oxide keeps.
pub const MAX_MEMORY_ITEMS: i64 = 300;

#[derive(sqlx::FromRow)]
struct MemoryRow {
    id: String,
    project_id: Option<String>,
    text: String,
    conversation_id: Option<String>,
    created_at: i64,
    updated_at: i64,
}

impl From<MemoryRow> for MemoryItem {
    fn from(r: MemoryRow) -> Self {
        MemoryItem {
            id: r.id,
            project_id: r.project_id,
            text: r.text,
            conversation_id: r.conversation_id,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

const COLUMNS: &str = "id, project_id, text, conversation_id, created_at, updated_at";
const NOT_FOUND: StoreError = StoreError::NotFound { what: "memory" };

impl Store {
    /// Every memory item, oldest first.
    pub async fn list_memory(&self) -> Result<Vec<MemoryItem>> {
        let rows: Vec<MemoryRow> = sqlx::query_as(&format!(
            "SELECT {COLUMNS} FROM memories ORDER BY created_at, id"
        ))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    /// The items a chat sees: global ones, plus its project's.
    pub async fn memory_for(&self, project_id: Option<&str>) -> Result<Vec<MemoryItem>> {
        let rows: Vec<MemoryRow> = sqlx::query_as(&format!(
            "SELECT {COLUMNS} FROM memories
             WHERE project_id IS NULL OR project_id = ?
             ORDER BY project_id IS NOT NULL, created_at, id"
        ))
        .bind(project_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    /// Adds an item. `conversation_id` is the chat it was learned from.
    pub async fn add_memory(
        &self,
        input: NewMemory,
        conversation_id: Option<&str>,
    ) -> Result<MemoryItem> {
        let text = clean_memory_text(&input.text)
            .ok_or_else(|| StoreError::Invalid("the memory is empty".into()))?;
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM memories")
            .fetch_one(&self.pool)
            .await?;
        if count >= MAX_MEMORY_ITEMS {
            return Err(StoreError::Invalid(format!(
                "memory holds at most {MAX_MEMORY_ITEMS} items; delete some first"
            )));
        }
        let now = now_ms();
        let item = MemoryItem {
            id: new_id(),
            project_id: input.project_id.filter(|p| !p.is_empty()),
            text,
            conversation_id: conversation_id.map(str::to_owned),
            created_at: now,
            updated_at: now,
        };
        sqlx::query(&format!(
            "INSERT INTO memories ({COLUMNS}) VALUES (?, ?, ?, ?, ?, ?)"
        ))
        .bind(&item.id)
        .bind(&item.project_id)
        .bind(&item.text)
        .bind(&item.conversation_id)
        .bind(item.created_at)
        .bind(item.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|e| map_fk(e, "project"))?;
        Ok(item)
    }

    pub async fn update_memory(&self, id: &str, text: &str) -> Result<MemoryItem> {
        let text = clean_memory_text(text)
            .ok_or_else(|| StoreError::Invalid("the memory is empty".into()))?;
        let done = sqlx::query("UPDATE memories SET text = ?, updated_at = ? WHERE id = ?")
            .bind(&text)
            .bind(now_ms())
            .bind(id)
            .execute(&self.pool)
            .await?;
        if done.rows_affected() == 0 {
            return Err(NOT_FOUND);
        }
        let row: MemoryRow =
            sqlx::query_as(&format!("SELECT {COLUMNS} FROM memories WHERE id = ?"))
                .bind(id)
                .fetch_one(&self.pool)
                .await?;
        Ok(row.into())
    }

    pub async fn delete_memory(&self, id: &str) -> Result<()> {
        let done = sqlx::query("DELETE FROM memories WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        if done.rows_affected() == 0 {
            return Err(NOT_FOUND);
        }
        Ok(())
    }

    /// Deletes every item (`Some(project)`: only that project's; `None`: all).
    pub async fn clear_memory(&self, project_id: Option<&str>) -> Result<u64> {
        let done = match project_id {
            Some(p) => {
                sqlx::query("DELETE FROM memories WHERE project_id = ?")
                    .bind(p)
                    .execute(&self.pool)
                    .await?
            }
            None => {
                sqlx::query("DELETE FROM memories")
                    .execute(&self.pool)
                    .await?
            }
        };
        Ok(done.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::store;
    use oxide_core::NewProject;

    fn new(project: Option<&str>, text: &str) -> NewMemory {
        NewMemory {
            project_id: project.map(str::to_owned),
            text: text.into(),
        }
    }

    #[tokio::test]
    async fn crud_scope_and_cascade() {
        let s = store().await;
        let p = s
            .create_project(NewProject {
                name: "P".into(),
                description: String::new(),
            })
            .await
            .expect("creates")
            .id;
        let g = s
            .add_memory(new(None, " - Prefers answers in Danish "), Some("chat-1"))
            .await
            .expect("adds");
        assert_eq!(g.text, "Prefers answers in Danish");
        assert_eq!(g.conversation_id.as_deref(), Some("chat-1"));
        let pm = s
            .add_memory(new(Some(&p), "Targets Windows first"), None)
            .await
            .expect("adds");
        assert!(matches!(
            s.add_memory(new(None, "  "), None).await,
            Err(StoreError::Invalid(_))
        ));
        assert!(matches!(
            s.add_memory(new(Some("nope"), "x"), None).await,
            Err(StoreError::NotFound { .. })
        ));

        assert_eq!(s.memory_for(None).await.expect("ok"), vec![g.clone()]);
        let in_project = s.memory_for(Some(&p)).await.expect("ok");
        assert_eq!(in_project.len(), 2);
        assert_eq!(in_project[0].id, g.id, "global items first");

        let edited = s
            .update_memory(&pm.id, "Targets Windows and macOS")
            .await
            .expect("ok");
        assert_eq!(edited.text, "Targets Windows and macOS");
        assert!(matches!(
            s.update_memory("nope", "x").await,
            Err(StoreError::NotFound { .. })
        ));

        s.delete_project(&p).await.expect("deletes");
        assert_eq!(s.list_memory().await.expect("ok"), vec![g.clone()]);
        s.delete_memory(&g.id).await.expect("deletes");
        assert!(s.list_memory().await.expect("ok").is_empty());
        s.add_memory(new(None, "a"), None).await.expect("adds");
        assert_eq!(s.clear_memory(None).await.expect("ok"), 1);
    }
}
