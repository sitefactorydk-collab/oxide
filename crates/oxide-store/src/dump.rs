//! Whole-database reads for Settings → Data: every table as JSON rows ("Export all") and the
//! ids "Delete all" needs before it removes the files.

use serde_json::{Map, Value};
use sqlx::sqlite::SqliteRow;
use sqlx::{Column, Row, TypeInfo, ValueRef};

use crate::{Result, Store};

/// One table's rows, each a JSON object keyed by column name.
#[derive(Debug, Clone, PartialEq)]
pub struct TableDump {
    pub name: String,
    pub rows: Vec<Map<String, Value>>,
}

impl Store {
    /// The tables holding Oxide's data, by name: not SQLite's own, not the migration log,
    /// and not full-text indexes (virtual tables and their shadow tables), which are
    /// rebuilt from the others.
    pub async fn data_tables(&self) -> Result<Vec<String>> {
        let rows: Vec<(String, Option<String>)> = sqlx::query_as(
            "SELECT name, sql FROM sqlite_master WHERE type = 'table' ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await?;
        let virtual_tables: Vec<String> = rows
            .iter()
            .filter(|(_, sql)| {
                sql.as_deref().is_some_and(|s| {
                    s.trim_start()
                        .to_ascii_uppercase()
                        .starts_with("CREATE VIRTUAL")
                })
            })
            .map(|(name, _)| name.clone())
            .collect();
        Ok(rows
            .into_iter()
            .map(|(name, _)| name)
            .filter(|name| {
                !name.starts_with("sqlite_")
                    && name != "_sqlx_migrations"
                    && !virtual_tables
                        .iter()
                        .any(|v| name == v || name.starts_with(&format!("{v}_")))
            })
            .collect())
    }

    /// Every data table with all its rows, read in one transaction so the export is a
    /// consistent snapshot. Integers, reals and text keep their types; blobs become
    /// `{"$hex": "…"}`.
    pub async fn dump_tables(&self) -> Result<Vec<TableDump>> {
        let names = self.data_tables().await?;
        let mut tx = self.pool.begin().await?;
        let mut out = Vec::with_capacity(names.len());
        for name in names {
            // `name` comes from sqlite_master; quoting keeps odd names safe anyway.
            let sql = format!("SELECT * FROM \"{}\"", name.replace('"', "\"\""));
            let rows: Vec<SqliteRow> = sqlx::query(&sql).fetch_all(&mut *tx).await?;
            let rows = rows.iter().map(row_json).collect::<Result<Vec<_>>>()?;
            out.push(TableDump { name, rows });
        }
        tx.commit().await?;
        Ok(out)
    }

    /// Every chat's id (for removing their sandboxes when all data is deleted).
    pub async fn all_conversation_ids(&self) -> Result<Vec<String>> {
        let ids: Vec<(String,)> = sqlx::query_as("SELECT id FROM conversations")
            .fetch_all(&self.pool)
            .await?;
        Ok(ids.into_iter().map(|(id,)| id).collect())
    }
}

fn row_json(row: &SqliteRow) -> Result<Map<String, Value>> {
    let mut map = Map::new();
    for (i, col) in row.columns().iter().enumerate() {
        let raw = row.try_get_raw(i)?;
        let value = if raw.is_null() {
            Value::Null
        } else {
            match raw.type_info().name() {
                "INTEGER" | "BOOLEAN" => Value::from(row.try_get::<i64, _>(i)?),
                "REAL" => row
                    .try_get::<f64, _>(i)
                    .map(|f| serde_json::Number::from_f64(f).map_or(Value::Null, Value::Number))?,
                "BLOB" => {
                    let bytes: Vec<u8> = row.try_get(i)?;
                    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
                    let mut m = Map::new();
                    m.insert("$hex".into(), Value::String(hex));
                    Value::Object(m)
                }
                _ => Value::String(row.try_get::<String, _>(i)?),
            }
        };
        map.insert(col.name().to_owned(), value);
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use oxide_core::{NewMemory, NewProject, StartConversation};

    use super::*;

    #[tokio::test]
    async fn dumps_every_data_table_and_skips_indexes() {
        let store = Store::open_in_memory().await.expect("opens");
        let tables = store.data_tables().await.expect("tables");
        for t in [
            "conversations",
            "messages",
            "settings",
            "providers",
            "scheduled_tasks",
        ] {
            assert!(tables.iter().any(|n| n == t), "{t} is exported");
        }
        assert!(!tables.iter().any(|n| n.contains("_fts")), "{tables:?}");
        assert!(
            !tables
                .iter()
                .any(|n| n.starts_with("sqlite_") || n == "_sqlx_migrations")
        );

        let project = store
            .create_project(NewProject {
                name: "Garden".into(),
                description: "Plants".into(),
            })
            .await
            .expect("project");
        let (chat, _) = store
            .start_conversation(StartConversation {
                text: "Tomatoes?".into(),
                model: "p/m".into(),
                project_id: Some(project.id.clone()),
                attachments: vec![],
                folders: vec![],
                tools: None,
            })
            .await
            .expect("chat");
        store
            .add_memory(
                NewMemory {
                    project_id: None,
                    text: "Likes tomatoes".into(),
                },
                None,
            )
            .await
            .expect("memory");

        let dump = store.dump_tables().await.expect("dumps");
        let get = |name: &str| {
            dump.iter()
                .find(|t| t.name == name)
                .unwrap_or_else(|| panic!("{name} dumped"))
        };
        let chats = get("conversations");
        assert_eq!(chats.rows.len(), 1);
        assert_eq!(chats.rows[0]["id"], Value::String(chat.id.clone()));
        assert_eq!(get("messages").rows.len(), 1);
        assert!(
            chats.rows[0]["created_at"].is_i64(),
            "integers stay numbers"
        );
        assert_eq!(get("projects").rows[0]["name"], "Garden");
        assert_eq!(get("memories").rows.len(), 1);

        assert_eq!(
            store.all_conversation_ids().await.expect("ids"),
            vec![chat.id]
        );
    }
}
