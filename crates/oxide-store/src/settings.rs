//! Settings: typed JSON documents in the `settings` table, one per key.

use oxide_core::AppSettings;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::{Result, Store, StoreError, now_ms};

/// Key of the [`AppSettings`] document.
const APP_SETTINGS_KEY: &str = "app";

impl Store {
    /// Loads a settings document, or `None` if it was never saved.
    ///
    /// A document that no longer parses is logged and treated as missing, so a bad value
    /// can't keep the app from starting.
    pub async fn get_setting<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        let raw: Option<String> = sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await?;
        Ok(raw.and_then(|json| match serde_json::from_str(&json) {
            Ok(v) => Some(v),
            Err(e) => {
                tracing::warn!(key, "ignoring unreadable settings document: {e}");
                None
            }
        }))
    }

    pub async fn put_setting<T: Serialize>(&self, key: &str, value: &T) -> Result<()> {
        let json = serde_json::to_string(value)
            .map_err(|e| StoreError::Invalid(format!("settings can't be serialized: {e}")))?;
        sqlx::query(
            "INSERT INTO settings (key, value, updated_at) VALUES (?, ?, ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        )
        .bind(key)
        .bind(json)
        .bind(now_ms())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn app_settings(&self) -> Result<AppSettings> {
        Ok(self
            .get_setting(APP_SETTINGS_KEY)
            .await?
            .unwrap_or_default())
    }

    /// Replaces the app settings and returns what was stored (normalized).
    pub async fn save_app_settings(&self, settings: AppSettings) -> Result<AppSettings> {
        let settings = settings.normalized();
        self.put_setting(APP_SETTINGS_KEY, &settings).await?;
        Ok(settings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::store;
    use oxide_core::{ChatFont, ThemePref};

    #[tokio::test]
    async fn defaults_until_saved_then_round_trips() {
        let s = store().await;
        assert_eq!(s.app_settings().await.expect("ok"), AppSettings::default());
        let mut v = AppSettings::default();
        v.profile.name = " Ada ".into();
        v.appearance.theme = ThemePref::Dark;
        v.appearance.chat_font = ChatFont::Sans;
        let saved = s.save_app_settings(v).await.expect("saves");
        assert_eq!(saved.profile.name, "Ada");
        assert_eq!(s.app_settings().await.expect("ok"), saved);
    }

    #[tokio::test]
    async fn unreadable_documents_fall_back_to_defaults() {
        let s = store().await;
        s.put_setting(APP_SETTINGS_KEY, &"not an object")
            .await
            .expect("ok");
        assert_eq!(s.app_settings().await.expect("ok"), AppSettings::default());
    }
}
