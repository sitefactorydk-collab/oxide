//! Cache of tool-calling probes for endpoints that don't report model capabilities.

use crate::{Result, Store, now_ms};

impl Store {
    /// The remembered probe result for `model` at `base_url`, if it was probed.
    pub async fn probe_result(&self, base_url: &str, model: &str) -> Result<Option<bool>> {
        let tools: Option<bool> = sqlx::query_scalar(
            "SELECT tools FROM capability_probes WHERE base_url = ? AND model = ?",
        )
        .bind(base_url)
        .bind(model)
        .fetch_optional(&self.pool)
        .await?;
        Ok(tools)
    }

    pub async fn save_probe_result(&self, base_url: &str, model: &str, tools: bool) -> Result<()> {
        sqlx::query(
            "INSERT INTO capability_probes (base_url, model, tools, checked_at) VALUES (?, ?, ?, ?)
             ON CONFLICT (base_url, model) DO UPDATE SET tools = excluded.tools,
                 checked_at = excluded.checked_at",
        )
        .bind(base_url)
        .bind(model)
        .bind(tools)
        .bind(now_ms())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Forgets every probe result for `base_url` (Settings → Models → Refresh).
    pub async fn clear_probe_results(&self, base_url: &str) -> Result<()> {
        sqlx::query("DELETE FROM capability_probes WHERE base_url = ?")
            .bind(base_url)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::test_util::store;

    #[tokio::test]
    async fn probe_results_are_remembered_and_cleared() {
        let s = store().await;
        let url = "http://127.0.0.1:47831/v1";
        assert_eq!(s.probe_result(url, "m").await.expect("ok"), None);
        s.save_probe_result(url, "m", false).await.expect("saves");
        s.save_probe_result(url, "m", true).await.expect("updates");
        assert_eq!(s.probe_result(url, "m").await.expect("ok"), Some(true));
        assert_eq!(
            s.probe_result("http://other/v1", "m").await.expect("ok"),
            None
        );
        s.clear_probe_results(url).await.expect("clears");
        assert_eq!(s.probe_result(url, "m").await.expect("ok"), None);
    }
}
