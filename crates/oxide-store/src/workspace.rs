//! Per-chat folder grants, workspace options and task lists (Phase 11).

use oxide_core::workspace::{MAX_CHAT_FOLDERS, MAX_TASKS};
use oxide_core::{ChatFolder, ChatWorkspace, TaskItem, TaskList};

use crate::{Result, Store, StoreError, now_ms};

/// The last part of a path, for display (`C:\a\Photos` → `Photos`).
pub fn folder_name(path: &str) -> String {
    let trimmed = path.trim_end_matches(['/', '\\']);
    let name = trimmed.rsplit(['/', '\\']).next().unwrap_or(trimmed);
    if name.is_empty() {
        path.to_owned()
    } else {
        name.to_owned()
    }
}

pub(crate) async fn insert_folder(
    conn: &mut sqlx::SqliteConnection,
    conversation_id: &str,
    path: &str,
) -> Result<()> {
    let path = path.trim();
    if path.is_empty() {
        return Err(StoreError::Invalid("the folder path is empty".into()));
    }
    let (count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM chat_folders WHERE conversation_id = ? AND path <> ?")
            .bind(conversation_id)
            .bind(path)
            .fetch_one(&mut *conn)
            .await?;
    if usize::try_from(count).unwrap_or(usize::MAX) >= MAX_CHAT_FOLDERS {
        return Err(StoreError::Invalid(format!(
            "a chat can have at most {MAX_CHAT_FOLDERS} folders"
        )));
    }
    sqlx::query(
        "INSERT INTO chat_folders (conversation_id, path, name, added_at) VALUES (?, ?, ?, ?)
         ON CONFLICT(conversation_id, path) DO NOTHING",
    )
    .bind(conversation_id)
    .bind(path)
    .bind(folder_name(path))
    .bind(now_ms())
    .execute(&mut *conn)
    .await?;
    Ok(())
}

impl Store {
    /// The chat's folders (in the order they were added) and options.
    pub async fn chat_workspace(&self, conversation_id: &str) -> Result<ChatWorkspace> {
        let rows: Vec<(String, String, i64)> = sqlx::query_as(
            "SELECT path, name, added_at FROM chat_folders WHERE conversation_id = ?
             ORDER BY added_at, path",
        )
        .bind(conversation_id)
        .fetch_all(&self.pool)
        .await?;
        let allow: Option<(bool,)> =
            sqlx::query_as("SELECT allow_delete FROM chat_workspace WHERE conversation_id = ?")
                .bind(conversation_id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(ChatWorkspace {
            folders: rows
                .into_iter()
                .map(|(path, name, added_at)| ChatFolder {
                    path,
                    name,
                    added_at,
                })
                .collect(),
            allow_delete: allow.is_some_and(|(a,)| a),
        })
    }

    /// Grants a folder (already resolved by the caller). Granting it again changes nothing.
    pub async fn add_chat_folder(
        &self,
        conversation_id: &str,
        path: &str,
    ) -> Result<ChatWorkspace> {
        self.get_conversation(conversation_id).await?;
        let mut conn = self.pool.acquire().await?;
        insert_folder(&mut conn, conversation_id, path).await?;
        drop(conn);
        self.chat_workspace(conversation_id).await
    }

    pub async fn remove_chat_folder(
        &self,
        conversation_id: &str,
        path: &str,
    ) -> Result<ChatWorkspace> {
        sqlx::query("DELETE FROM chat_folders WHERE conversation_id = ? AND path = ?")
            .bind(conversation_id)
            .bind(path)
            .execute(&self.pool)
            .await?;
        self.chat_workspace(conversation_id).await
    }

    pub async fn set_chat_allow_delete(
        &self,
        conversation_id: &str,
        allow: bool,
    ) -> Result<ChatWorkspace> {
        self.get_conversation(conversation_id).await?;
        sqlx::query(
            "INSERT INTO chat_workspace (conversation_id, allow_delete) VALUES (?, ?)
             ON CONFLICT(conversation_id) DO UPDATE SET allow_delete = excluded.allow_delete",
        )
        .bind(conversation_id)
        .bind(allow)
        .execute(&self.pool)
        .await?;
        self.chat_workspace(conversation_id).await
    }

    /// The chat's task list (empty if the model never set one).
    pub async fn task_list(&self, conversation_id: &str) -> Result<TaskList> {
        let row: Option<(String, i64)> =
            sqlx::query_as("SELECT items, updated_at FROM chat_tasks WHERE conversation_id = ?")
                .bind(conversation_id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(match row {
            Some((json, updated_at)) => TaskList {
                conversation_id: conversation_id.to_owned(),
                items: serde_json::from_str(&json).unwrap_or_default(),
                updated_at,
            },
            None => TaskList {
                conversation_id: conversation_id.to_owned(),
                ..TaskList::default()
            },
        })
    }

    /// Replaces the chat's task list.
    pub async fn set_task_list(
        &self,
        conversation_id: &str,
        items: &[TaskItem],
    ) -> Result<TaskList> {
        if items.len() > MAX_TASKS {
            return Err(StoreError::Invalid(format!(
                "a task list can have at most {MAX_TASKS} steps"
            )));
        }
        self.get_conversation(conversation_id).await?;
        let json = serde_json::to_string(items).map_err(|e| StoreError::Corrupt(e.to_string()))?;
        let now = now_ms();
        sqlx::query(
            "INSERT INTO chat_tasks (conversation_id, items, updated_at) VALUES (?, ?, ?)
             ON CONFLICT(conversation_id) DO UPDATE SET items = excluded.items,
             updated_at = excluded.updated_at",
        )
        .bind(conversation_id)
        .bind(json)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(TaskList {
            conversation_id: conversation_id.to_owned(),
            items: items.to_vec(),
            updated_at: now,
        })
    }
}

#[cfg(test)]
mod tests {
    use oxide_core::{StartConversation, TaskItem, TaskStatus};

    use super::folder_name;
    use crate::Store;

    async fn chat(store: &Store, folders: Vec<String>) -> String {
        store
            .start_conversation(StartConversation {
                text: "hi".into(),
                model: "p/m".into(),
                project_id: None,
                attachments: vec![],
                folders,
                tools: None,
            })
            .await
            .expect("chat")
            .0
            .id
    }

    #[test]
    fn folder_names_are_the_last_part() {
        assert_eq!(folder_name(r"C:\AI Apps\Photos\"), "Photos");
        assert_eq!(folder_name("/home/a/docs"), "docs");
        assert_eq!(folder_name("/"), "/");
    }

    #[tokio::test]
    async fn folders_are_granted_with_the_chat_and_later() {
        let store = Store::open_in_memory().await.expect("store");
        let id = chat(&store, vec!["/data/one".into()]).await;
        let ws = store.chat_workspace(&id).await.expect("ws");
        assert_eq!(ws.folders.len(), 1);
        assert_eq!(ws.folders[0].name, "one");
        assert!(!ws.allow_delete);
        store.add_chat_folder(&id, "/data/two").await.expect("adds");
        let ws = store
            .add_chat_folder(&id, "/data/two")
            .await
            .expect("again");
        assert_eq!(ws.folders.len(), 2);
        let ws = store.set_chat_allow_delete(&id, true).await.expect("allow");
        assert!(ws.allow_delete);
        let ws = store
            .remove_chat_folder(&id, "/data/one")
            .await
            .expect("removes");
        assert_eq!(ws.folders.len(), 1);
        assert!(store.add_chat_folder("nope", "/x").await.is_err());
        assert!(store.add_chat_folder(&id, "  ").await.is_err());
        store.delete_conversation(&id).await.expect("deletes");
        assert!(
            store
                .chat_workspace(&id)
                .await
                .expect("ws")
                .folders
                .is_empty()
        );
    }

    #[tokio::test]
    async fn folder_count_is_limited() {
        let store = Store::open_in_memory().await.expect("store");
        let id = chat(&store, vec![]).await;
        for i in 0..oxide_core::workspace::MAX_CHAT_FOLDERS {
            store
                .add_chat_folder(&id, &format!("/f{i}"))
                .await
                .expect("adds");
        }
        assert!(store.add_chat_folder(&id, "/one-more").await.is_err());
        // Granting one it has again is fine.
        store.add_chat_folder(&id, "/f0").await.expect("same");
    }

    #[tokio::test]
    async fn task_lists_are_replaced_and_cascade() {
        let store = Store::open_in_memory().await.expect("store");
        let id = chat(&store, vec![]).await;
        assert!(store.task_list(&id).await.expect("list").items.is_empty());
        let items = vec![
            TaskItem {
                title: "Look".into(),
                status: TaskStatus::Done,
            },
            TaskItem {
                title: "Sort".into(),
                status: TaskStatus::InProgress,
            },
        ];
        store.set_task_list(&id, &items).await.expect("sets");
        let list = store.task_list(&id).await.expect("list");
        assert_eq!(list.items, items);
        assert_eq!(list.progress(), (1, 2));
        let many = vec![items[0].clone(); oxide_core::workspace::MAX_TASKS + 1];
        assert!(store.set_task_list(&id, &many).await.is_err());
        store.delete_conversation(&id).await.expect("deletes");
        assert!(store.task_list(&id).await.expect("list").items.is_empty());
    }
}
