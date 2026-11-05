//! Provider endpoints and their API keys.
//!
//! Keys only ever travel frontend → backend (`set_provider_key`, `test_provider`). Responses
//! carry a masked hint at most, and keys are never logged.

use std::time::Instant;

use oxide_core::{ProviderConfig, ProviderInput, ProviderTestResult, TestProvider};
use oxide_llm::probe::{self, ProbeError};
use oxide_store::Secret;
use tauri::State;

use crate::error::CmdResult;
use crate::state::AppState;

/// Most model ids returned by a connection test.
const SAMPLE_MODELS: usize = 20;

#[tauri::command]
#[specta::specta]
pub async fn list_providers(state: State<'_, AppState>) -> CmdResult<Vec<ProviderConfig>> {
    Ok(state.store.list_providers().await?)
}

/// Creates (`id: null`) or updates an endpoint.
#[tauri::command]
#[specta::specta]
pub async fn save_provider(
    state: State<'_, AppState>,
    input: ProviderInput,
) -> CmdResult<ProviderConfig> {
    let saved = state.store.save_provider(input).await?;
    state.models.invalidate();
    Ok(saved)
}

/// Deletes an endpoint and its key.
#[tauri::command]
#[specta::specta]
pub async fn delete_provider(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    state.store.delete_provider(&state.secrets, &id).await?;
    state.models.invalidate();
    Ok(())
}

/// Stores an API key in the OS keychain. Returns the endpoint with the key's masked hint.
#[tauri::command]
#[specta::specta]
pub async fn set_provider_key(
    state: State<'_, AppState>,
    id: String,
    key: String,
) -> CmdResult<ProviderConfig> {
    let config = state
        .store
        .set_provider_key(&state.secrets, &id, Secret::new(key))
        .await?;
    state.models.invalidate();
    tracing::info!(provider = %id, "API key stored in the keychain");
    Ok(config)
}

#[tauri::command]
#[specta::specta]
pub async fn clear_provider_key(
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<ProviderConfig> {
    let config = state.store.clear_provider_key(&state.secrets, &id).await?;
    state.models.invalidate();
    tracing::info!(provider = %id, "API key removed from the keychain");
    Ok(config)
}

/// Lists the endpoint's models to check the URL and key. Uses `apiKey` when given, otherwise
/// the stored key of `providerId`. Failures come back as `ok: false` with a readable message.
#[tauri::command]
#[specta::specta]
pub async fn test_provider(
    state: State<'_, AppState>,
    input: TestProvider,
) -> CmdResult<ProviderTestResult> {
    let typed = input
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|k| !k.is_empty())
        .map(Secret::new);
    let key = match (typed, input.provider_id.as_deref()) {
        (Some(k), _) => Some(k),
        (None, Some(id)) => state.store.provider_key(&state.secrets, id).await?,
        (None, None) => None,
    };

    let started = Instant::now();
    let outcome = probe::list_models(
        input.kind,
        &input.base_url,
        key.as_ref().map(Secret::expose),
    )
    .await;
    let latency_ms = u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX);
    Ok(describe(input.kind, key.is_some(), outcome, latency_ms))
}

fn describe(
    kind: oxide_core::ProviderKind,
    had_key: bool,
    outcome: Result<Vec<String>, ProbeError>,
    latency_ms: u32,
) -> ProviderTestResult {
    match outcome {
        Ok(models) => {
            let count = models.len();
            let message = if count == 0 {
                "Connected, but the server lists no models.".to_owned()
            } else {
                format!(
                    "Connected. {count} {} available.",
                    if count == 1 { "model" } else { "models" }
                )
            };
            ProviderTestResult {
                ok: true,
                message,
                model_count: u32::try_from(count).unwrap_or(u32::MAX),
                models: models.into_iter().take(SAMPLE_MODELS).collect(),
                latency_ms,
            }
        }
        Err(err) => {
            let mut message = err.to_string();
            if matches!(err, ProbeError::Unauthorized { .. }) && !had_key && kind.requires_key() {
                message = format!("{} needs an API key.", kind.label());
            }
            ProviderTestResult {
                ok: false,
                message,
                model_count: 0,
                models: Vec::new(),
                latency_ms,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxide_core::ProviderKind;

    #[test]
    fn success_messages_count_models_and_cap_the_sample() {
        let models: Vec<String> = (0..30).map(|i| format!("m{i}")).collect();
        let r = describe(ProviderKind::OpenAi, true, Ok(models), 12);
        assert!(r.ok);
        assert_eq!(r.model_count, 30);
        assert_eq!(r.models.len(), SAMPLE_MODELS);
        assert_eq!(r.message, "Connected. 30 models available.");
    }

    #[test]
    fn missing_key_gets_a_helpful_message() {
        let r = describe(
            ProviderKind::Anthropic,
            false,
            Err(ProbeError::Unauthorized { status: 401 }),
            5,
        );
        assert!(!r.ok);
        assert_eq!(r.message, "Anthropic needs an API key.");
    }
}
