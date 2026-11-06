//! Glue between `oxide-mcp` and the app: MCP events to the frontend, sampling through
//! Oxide's models, links to the system browser, and sign-ins in the OS keychain.

use std::sync::Arc;

use futures_util::future::BoxFuture;
use oxide_agent::ModelRegistry;
use oxide_core::McpEvent;
use oxide_mcp::oauth::TokenStore;
use oxide_mcp::{ManagerSink, UserAgent};
use oxide_store::SecretStore;
use serde_json::Value;
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;
use tauri_specta::Event;

use crate::events::McpChanged;

/// Sends MCP events to every window (`events.mcpChanged`).
pub struct TauriMcpSink {
    app: AppHandle,
}

impl TauriMcpSink {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl ManagerSink for TauriMcpSink {
    fn event(&self, event: McpEvent) {
        if let Err(e) = McpChanged(event).emit(&self.app) {
            tracing::warn!("couldn't send an MCP event: {e}");
        }
    }
}

/// Opens web links in the system browser.
pub fn open_in_browser(app: &AppHandle, url: &str) -> Result<(), String> {
    let lower = url.to_ascii_lowercase();
    if !(lower.starts_with("https://") || lower.starts_with("http://")) {
        return Err("only web links can be opened".into());
    }
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| format!("couldn't open the browser: {e}"))
}

/// What servers may ask of the app.
pub struct AppAgent {
    app: AppHandle,
    models: Arc<ModelRegistry>,
}

impl AppAgent {
    pub fn new(app: AppHandle, models: Arc<ModelRegistry>) -> Self {
        Self { app, models }
    }
}

impl UserAgent for AppAgent {
    fn sample(&self, params: Value, model: Option<String>) -> BoxFuture<'_, Result<Value, String>> {
        Box::pin(async move {
            let model = match model.filter(|m| !m.trim().is_empty()) {
                Some(m) => m,
                None => {
                    // The first model that can be used, in the endpoints' order.
                    let listed = self.models.list(false).await.map_err(|e| e.to_string())?;
                    listed
                        .iter()
                        .flat_map(|p| p.models.iter())
                        .find(|m| m.capabilities.tools)
                        .map(|m| m.id.clone())
                        .ok_or_else(|| "no model is available to answer".to_owned())?
                }
            };
            oxide_agent::run_sampling(&self.models, &model, &params)
                .await
                .map_err(|e| e.to_string())
        })
    }

    fn open_url(&self, url: &str) -> Result<(), String> {
        open_in_browser(&self.app, url)
    }
}

/// OAuth sign-ins in the OS keychain (split over several entries when long).
pub struct KeychainTokens {
    secrets: Arc<dyn SecretStore>,
}

impl KeychainTokens {
    pub fn new(secrets: Arc<dyn SecretStore>) -> Self {
        Self { secrets }
    }
}

impl TokenStore for KeychainTokens {
    fn load<'a>(&'a self, key: &'a str) -> BoxFuture<'a, Result<Option<String>, String>> {
        let secrets = self.secrets.clone();
        let key = key.to_owned();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || oxide_store::get_large(secrets.as_ref(), &key))
                .await
                .map_err(|e| e.to_string())?
                .map(|s| s.map(|s| s.expose().to_owned()))
                .map_err(|e| e.to_string())
        })
    }

    fn save<'a>(&'a self, key: &'a str, value: String) -> BoxFuture<'a, Result<(), String>> {
        let secrets = self.secrets.clone();
        let key = key.to_owned();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || {
                oxide_store::set_large(secrets.as_ref(), &key, &value)
            })
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())
        })
    }

    fn delete<'a>(&'a self, key: &'a str) -> BoxFuture<'a, Result<(), String>> {
        let secrets = self.secrets.clone();
        let key = key.to_owned();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || oxide_store::delete_large(secrets.as_ref(), &key))
                .await
                .map_err(|e| e.to_string())?
                .map_err(|e| e.to_string())
        })
    }
}
