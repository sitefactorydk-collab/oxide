//! Provider endpoints. Rows hold the endpoint and a masked key hint; the key itself goes to
//! a [`SecretStore`] under `provider:<id>`.

use std::sync::Arc;

use oxide_core::provider::{mask_key, normalize_base_url};
use oxide_core::{ProviderConfig, ProviderInput, ProviderKind};

use crate::secrets::{Secret, SecretStore};
use crate::{Result, Store, StoreError, new_id, now_ms};

#[derive(sqlx::FromRow)]
struct ProviderRow {
    id: String,
    kind: String,
    name: String,
    base_url: String,
    enabled: bool,
    key_hint: Option<String>,
    created_at: i64,
    updated_at: i64,
}

impl TryFrom<ProviderRow> for ProviderConfig {
    type Error = StoreError;

    fn try_from(r: ProviderRow) -> Result<Self> {
        let kind = ProviderKind::parse(&r.kind)
            .ok_or_else(|| StoreError::Corrupt(format!("unknown provider kind {}", r.kind)))?;
        Ok(ProviderConfig {
            id: r.id,
            kind,
            name: r.name,
            base_url: r.base_url,
            enabled: r.enabled,
            key_hint: r.key_hint,
            created_at: r.created_at,
            updated_at: r.updated_at,
        })
    }
}

const COLUMNS: &str = "id, kind, name, base_url, enabled, key_hint, created_at, updated_at";
const NOT_FOUND: StoreError = StoreError::NotFound { what: "provider" };

/// Name of a provider's key in the secret store.
pub fn secret_name(provider_id: &str) -> String {
    format!("provider:{provider_id}")
}

fn validate(input: &ProviderInput) -> Result<(String, String)> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(StoreError::Invalid("name is empty".into()));
    }
    let url = normalize_base_url(&input.base_url);
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(StoreError::Invalid(
            "the base URL must start with http:// or https://".into(),
        ));
    }
    Ok((name.chars().take(80).collect(), url))
}

/// Runs a blocking secret-store call off the async runtime.
async fn blocking<T: Send + 'static>(
    secrets: &Arc<dyn SecretStore>,
    f: impl FnOnce(&dyn SecretStore) -> Result<T, crate::SecretError> + Send + 'static,
) -> Result<T> {
    let secrets = Arc::clone(secrets);
    Ok(tokio::task::spawn_blocking(move || f(secrets.as_ref())).await??)
}

impl Store {
    /// All endpoints in display order.
    pub async fn list_providers(&self) -> Result<Vec<ProviderConfig>> {
        let rows: Vec<ProviderRow> = sqlx::query_as(&format!(
            "SELECT {COLUMNS} FROM providers ORDER BY position, created_at"
        ))
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(ProviderConfig::try_from).collect()
    }

    pub async fn get_provider(&self, id: &str) -> Result<ProviderConfig> {
        let row: Option<ProviderRow> =
            sqlx::query_as(&format!("SELECT {COLUMNS} FROM providers WHERE id = ?"))
                .bind(id)
                .fetch_optional(&self.pool)
                .await?;
        row.ok_or(NOT_FOUND)?.try_into()
    }

    /// Creates an endpoint (`input.id` is `None`) or updates one.
    pub async fn save_provider(&self, input: ProviderInput) -> Result<ProviderConfig> {
        let (name, base_url) = validate(&input)?;
        let now = now_ms();
        match input.id.as_deref() {
            Some(id) => {
                let done = sqlx::query(
                    "UPDATE providers SET kind = ?, name = ?, base_url = ?, enabled = ?, updated_at = ?
                     WHERE id = ?",
                )
                .bind(input.kind.as_str())
                .bind(&name)
                .bind(&base_url)
                .bind(input.enabled)
                .bind(now)
                .bind(id)
                .execute(&self.pool)
                .await?;
                if done.rows_affected() == 0 {
                    return Err(NOT_FOUND);
                }
                self.get_provider(id).await
            }
            None => {
                let id = new_id();
                sqlx::query(
                    "INSERT INTO providers (id, kind, name, base_url, enabled, position, created_at, updated_at)
                     VALUES (?, ?, ?, ?, ?, (SELECT COALESCE(MAX(position), -1) + 1 FROM providers), ?, ?)",
                )
                .bind(&id)
                .bind(input.kind.as_str())
                .bind(&name)
                .bind(&base_url)
                .bind(input.enabled)
                .bind(now)
                .bind(now)
                .execute(&self.pool)
                .await?;
                self.get_provider(&id).await
            }
        }
    }

    /// Deletes an endpoint and its stored key.
    pub async fn delete_provider(&self, secrets: &Arc<dyn SecretStore>, id: &str) -> Result<()> {
        self.get_provider(id).await?;
        let name = secret_name(id);
        blocking(secrets, move |s| s.delete(&name)).await?;
        sqlx::query("DELETE FROM providers WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Stores an API key in the secret store and records its masked hint.
    pub async fn set_provider_key(
        &self,
        secrets: &Arc<dyn SecretStore>,
        id: &str,
        key: Secret,
    ) -> Result<ProviderConfig> {
        let trimmed = key.expose().trim();
        if trimmed.is_empty() {
            return Err(StoreError::Invalid("the API key is empty".into()));
        }
        self.get_provider(id).await?;
        let hint = mask_key(trimmed);
        let key = Secret::new(trimmed);
        let name = secret_name(id);
        blocking(secrets, move |s| s.set(&name, &key)).await?;
        self.set_key_hint(id, Some(hint)).await
    }

    pub async fn clear_provider_key(
        &self,
        secrets: &Arc<dyn SecretStore>,
        id: &str,
    ) -> Result<ProviderConfig> {
        self.get_provider(id).await?;
        let name = secret_name(id);
        blocking(secrets, move |s| s.delete(&name)).await?;
        self.set_key_hint(id, None).await
    }

    /// The stored key for an endpoint, if any.
    pub async fn provider_key(
        &self,
        secrets: &Arc<dyn SecretStore>,
        id: &str,
    ) -> Result<Option<Secret>> {
        let name = secret_name(id);
        blocking(secrets, move |s| s.get(&name)).await
    }

    async fn set_key_hint(&self, id: &str, hint: Option<String>) -> Result<ProviderConfig> {
        sqlx::query("UPDATE providers SET key_hint = ?, updated_at = ? WHERE id = ?")
            .bind(hint)
            .bind(now_ms())
            .bind(id)
            .execute(&self.pool)
            .await?;
        self.get_provider(id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::MemorySecrets;
    use crate::test_util::store;

    fn secrets() -> Arc<dyn SecretStore> {
        Arc::new(MemorySecrets::default())
    }

    #[tokio::test]
    async fn defaults_are_seeded() {
        let s = store().await;
        let list = s.list_providers().await.expect("ok");
        let kinds: Vec<_> = list.iter().map(|p| p.kind).collect();
        assert_eq!(kinds, ProviderKind::ALL.to_vec());
        for p in &list {
            assert_eq!(p.base_url, p.kind.default_base_url());
            assert_eq!(p.key_hint, None);
        }
    }

    #[tokio::test]
    async fn create_update_and_validate() {
        let s = store().await;
        let p = s
            .save_provider(ProviderInput {
                id: None,
                kind: ProviderKind::OpenAi,
                name: "Work proxy".into(),
                base_url: "https://proxy.example/v1/".into(),
                enabled: true,
            })
            .await
            .expect("creates");
        assert_eq!(p.base_url, "https://proxy.example/v1");
        assert_eq!(s.list_providers().await.expect("ok").last(), Some(&p));

        let updated = s
            .save_provider(ProviderInput {
                id: Some(p.id.clone()),
                kind: ProviderKind::OpenAi,
                name: "Proxy".into(),
                base_url: p.base_url.clone(),
                enabled: false,
            })
            .await
            .expect("updates");
        assert_eq!(updated.name, "Proxy");
        assert!(!updated.enabled);

        let bad = s
            .save_provider(ProviderInput {
                id: None,
                kind: ProviderKind::Strata,
                name: "x".into(),
                base_url: "ftp://x".into(),
                enabled: true,
            })
            .await;
        assert!(matches!(bad, Err(StoreError::Invalid(_))));
    }

    #[tokio::test]
    async fn keys_go_to_the_secret_store_with_only_a_hint_in_the_db() {
        let s = store().await;
        let sec = secrets();
        let key = "sk-ant-test-0123456789abcd";
        let p = s
            .set_provider_key(&sec, "anthropic", Secret::new(format!("  {key} ")))
            .await
            .expect("sets");
        assert_eq!(p.key_hint.as_deref(), Some("••••abcd"));
        assert_eq!(
            s.provider_key(&sec, "anthropic").await.expect("ok"),
            Some(Secret::new(key))
        );

        // Nothing in the database contains the key.
        let leaked: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM providers WHERE key_hint LIKE '%0123456789%' OR base_url LIKE '%sk-ant%'",
        )
        .fetch_one(&s.pool)
        .await
        .expect("query");
        assert_eq!(leaked, 0);

        let cleared = s
            .clear_provider_key(&sec, "anthropic")
            .await
            .expect("clears");
        assert_eq!(cleared.key_hint, None);
        assert_eq!(s.provider_key(&sec, "anthropic").await.expect("ok"), None);
    }

    #[tokio::test]
    async fn delete_removes_row_and_key() {
        let s = store().await;
        let sec = secrets();
        s.set_provider_key(&sec, "openai", Secret::new("sk-proj-abcdefghijkl"))
            .await
            .expect("sets");
        s.delete_provider(&sec, "openai").await.expect("deletes");
        assert!(matches!(
            s.get_provider("openai").await,
            Err(StoreError::NotFound { .. })
        ));
        assert_eq!(sec.get(&secret_name("openai")).expect("ok"), None);
    }

    #[tokio::test]
    async fn empty_keys_and_unknown_providers_are_rejected() {
        let s = store().await;
        let sec = secrets();
        assert!(matches!(
            s.set_provider_key(&sec, "openai", Secret::new("  ")).await,
            Err(StoreError::Invalid(_))
        ));
        assert!(matches!(
            s.set_provider_key(&sec, "nope", Secret::new("sk-abcdefghijklmn"))
                .await,
            Err(StoreError::NotFound { .. })
        ));
    }
}
