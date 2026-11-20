//! Utility Apps' settings and storage (Phase 20). The apps' files live in folders; see
//! `oxide-agent`'s `apps` module.

use oxide_core::AppPrefs;

use crate::{Result, Store, StoreError, now_ms};

impl Store {
    /// An app's settings (defaults when none are saved).
    pub async fn app_prefs(&self, app_id: &str) -> Result<AppPrefs> {
        let raw: Option<String> =
            sqlx::query_scalar("SELECT prefs FROM app_prefs WHERE app_id = ?")
                .bind(app_id)
                .fetch_optional(&self.pool)
                .await?;
        match raw {
            None => Ok(AppPrefs::default()),
            Some(s) => serde_json::from_str(&s)
                .map_err(|e| StoreError::Corrupt(format!("app settings of {app_id}: {e}"))),
        }
    }

    pub async fn save_app_prefs(&self, app_id: &str, prefs: &AppPrefs) -> Result<()> {
        let json = serde_json::to_string(prefs)
            .map_err(|e| StoreError::Corrupt(format!("app settings: {e}")))?;
        sqlx::query(
            "INSERT INTO app_prefs (app_id, prefs, updated_at) VALUES (?, ?, ?)
             ON CONFLICT (app_id) DO UPDATE SET prefs = excluded.prefs,
                 updated_at = excluded.updated_at",
        )
        .bind(app_id)
        .bind(json)
        .bind(now_ms())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Forgets an app's settings and storage (the app was deleted).
    pub async fn delete_app_data(&self, app_id: &str) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM app_prefs WHERE app_id = ?")
            .bind(app_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM app_storage WHERE app_id = ?")
            .bind(app_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn app_storage_get(&self, app_id: &str, key: &str) -> Result<Option<String>> {
        Ok(
            sqlx::query_scalar("SELECT value FROM app_storage WHERE app_id = ? AND key = ?")
                .bind(app_id)
                .bind(key)
                .fetch_optional(&self.pool)
                .await?,
        )
    }

    /// Sets a value. Refused with [`StoreError::Invalid`] when the app's storage would grow
    /// past `max_bytes` (keys and values counted).
    pub async fn app_storage_set(
        &self,
        app_id: &str,
        key: &str,
        value: &str,
        max_bytes: u64,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        let used: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(LENGTH(CAST(key AS BLOB)) + LENGTH(CAST(value AS BLOB))), 0)
             FROM app_storage WHERE app_id = ? AND key != ?",
        )
        .bind(app_id)
        .bind(key)
        .fetch_one(&mut *tx)
        .await?;
        let after = u64::try_from(used).unwrap_or(0) + (key.len() + value.len()) as u64;
        if after > max_bytes {
            return Err(StoreError::Invalid(format!(
                "the app's storage is full ({} MB at most)",
                max_bytes / (1024 * 1024)
            )));
        }
        sqlx::query(
            "INSERT INTO app_storage (app_id, key, value, updated_at) VALUES (?, ?, ?, ?)
             ON CONFLICT (app_id, key) DO UPDATE SET value = excluded.value,
                 updated_at = excluded.updated_at",
        )
        .bind(app_id)
        .bind(key)
        .bind(value)
        .bind(now_ms())
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn app_storage_delete(&self, app_id: &str, key: &str) -> Result<()> {
        sqlx::query("DELETE FROM app_storage WHERE app_id = ? AND key = ?")
            .bind(app_id)
            .bind(key)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn app_storage_keys(&self, app_id: &str) -> Result<Vec<String>> {
        Ok(
            sqlx::query_scalar("SELECT key FROM app_storage WHERE app_id = ? ORDER BY key")
                .bind(app_id)
                .fetch_all(&self.pool)
                .await?,
        )
    }

    pub async fn app_storage_clear(&self, app_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM app_storage WHERE app_id = ?")
            .bind(app_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Bytes an app keeps in its storage.
    pub async fn app_storage_bytes(&self, app_id: &str) -> Result<u64> {
        let used: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(LENGTH(CAST(key AS BLOB)) + LENGTH(CAST(value AS BLOB))), 0)
             FROM app_storage WHERE app_id = ?",
        )
        .bind(app_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(u64::try_from(used).unwrap_or(0))
    }
}

#[cfg(test)]
mod tests {
    use oxide_core::{AppCapability, AppPrefs};

    use crate::StoreError;
    use crate::test_util::store;

    #[tokio::test]
    async fn prefs_round_trip_and_are_deleted_with_the_app() {
        let s = store().await;
        assert_eq!(s.app_prefs("a").await.expect("ok"), AppPrefs::default());
        let prefs = AppPrefs {
            text_model: Some("openai/gpt-5".into()),
            granted: vec![AppCapability::Text, AppCapability::WebFetch],
            ..AppPrefs::default()
        };
        s.save_app_prefs("a", &prefs).await.expect("saves");
        assert_eq!(s.app_prefs("a").await.expect("ok"), prefs);
        s.app_storage_set("a", "k", "\"v\"", 1_000)
            .await
            .expect("sets");
        s.delete_app_data("a").await.expect("deletes");
        assert_eq!(s.app_prefs("a").await.expect("ok"), AppPrefs::default());
        assert_eq!(s.app_storage_get("a", "k").await.expect("ok"), None);
    }

    #[tokio::test]
    async fn storage_is_per_app_and_capped() {
        let s = store().await;
        s.app_storage_set("a", "x", "1", 100).await.expect("sets");
        s.app_storage_set("a", "y", "2", 100).await.expect("sets");
        s.app_storage_set("b", "x", "3", 100).await.expect("sets");
        s.app_storage_set("a", "x", "10", 100)
            .await
            .expect("replaces");
        assert_eq!(
            s.app_storage_get("a", "x").await.expect("ok").as_deref(),
            Some("10")
        );
        assert_eq!(s.app_storage_keys("a").await.expect("ok"), vec!["x", "y"]);
        assert_eq!(s.app_storage_bytes("a").await.expect("ok"), 5);
        let full = s.app_storage_set("a", "z", &"v".repeat(200), 100).await;
        assert!(matches!(full, Err(StoreError::Invalid(_))), "{full:?}");
        // Replacing a value counts the new value, not both.
        s.app_storage_set("a", "x", &"v".repeat(90), 100)
            .await
            .expect("fits");
        s.app_storage_delete("a", "y").await.expect("deletes");
        s.app_storage_clear("b").await.expect("clears");
        assert!(s.app_storage_keys("b").await.expect("ok").is_empty());
        assert_eq!(s.app_storage_keys("a").await.expect("ok"), vec!["x"]);
    }
}
