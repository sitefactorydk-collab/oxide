//! The provider abstraction and endpoint description.

use std::sync::Arc;

use futures_util::StreamExt;
use futures_util::future::BoxFuture;
use oxide_core::provider::normalize_base_url;
use oxide_core::{ModelInfo, ProviderKind};

use crate::anthropic::AnthropicProvider;
use crate::openai::OpenAiCompatProvider;
use crate::{ChatEvent, ChatRequest, ChatStream, LlmError};

/// Where and how to reach one configured endpoint.
#[derive(Clone)]
pub struct Endpoint {
    /// The endpoint's id in Oxide (the `<providerId>` part of model ids).
    pub id: String,
    pub kind: ProviderKind,
    /// Normalized base URL ending at the API version (`…/v1`).
    pub base_url: String,
    api_key: Option<String>,
}

impl Endpoint {
    pub fn new(
        id: impl Into<String>,
        kind: ProviderKind,
        base_url: &str,
        api_key: Option<String>,
    ) -> Self {
        Self {
            id: id.into(),
            kind,
            base_url: normalize_base_url(base_url),
            api_key: api_key
                .map(|k| k.trim().to_owned())
                .filter(|k| !k.is_empty()),
        }
    }

    pub(crate) fn api_key(&self) -> Option<&str> {
        self.api_key.as_deref()
    }

    pub fn has_key(&self) -> bool {
        self.api_key.is_some()
    }
}

impl std::fmt::Debug for Endpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Endpoint")
            .field("id", &self.id)
            .field("kind", &self.kind)
            .field("base_url", &self.base_url)
            .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
            .finish()
    }
}

/// The models of an endpoint. `unverified` lists models whose tool support the endpoint
/// doesn't report; callers check them with [`Provider::probe_tools`] (and cache the result).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModelListing {
    pub models: Vec<ModelInfo>,
    pub unverified: Vec<String>,
}

/// A model API.
pub trait Provider: Send + Sync {
    fn endpoint(&self) -> &Endpoint;

    /// Chat models with their capabilities. Non-chat models (embeddings, speech, images)
    /// are left out.
    fn list_models(&self) -> BoxFuture<'_, Result<ModelListing, LlmError>>;

    /// Starts a streamed reply. Errors before the first event (bad key, unknown model,
    /// rate limits after retries) come back here; later ones arrive in the stream.
    fn stream(&self, request: ChatRequest) -> BoxFuture<'_, Result<ChatStream, LlmError>>;

    /// Checks with a tiny real request whether `model` makes native tool calls.
    fn probe_tools<'a>(&'a self, model: &'a str) -> BoxFuture<'a, Result<bool, LlmError>>;
}

/// The client for an endpoint's API family.
pub fn connect(endpoint: Endpoint) -> Result<Arc<dyn Provider>, LlmError> {
    if !(endpoint.base_url.starts_with("http://") || endpoint.base_url.starts_with("https://")) {
        return Err(LlmError::Invalid(
            "the base URL must start with http:// or https://".into(),
        ));
    }
    Ok(match endpoint.kind {
        ProviderKind::Anthropic => Arc::new(AnthropicProvider::new(endpoint)?),
        ProviderKind::OpenAi | ProviderKind::LmStudio | ProviderKind::Strata => {
            Arc::new(OpenAiCompatProvider::new(endpoint)?)
        }
    })
}

/// Runs a request and returns just its text (used for short jobs such as titles).
pub async fn complete_text(
    provider: &dyn Provider,
    request: ChatRequest,
) -> Result<String, LlmError> {
    let mut stream = provider.stream(request).await?;
    let mut text = String::new();
    while let Some(event) = stream.next().await {
        if let ChatEvent::TextDelta(t) = event? {
            text.push_str(&t);
        }
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_output_hides_the_key() {
        let e = Endpoint::new(
            "a",
            ProviderKind::Anthropic,
            "https://x/v1/",
            Some("sk-secret".into()),
        );
        let text = format!("{e:?}");
        assert!(!text.contains("sk-secret"));
        assert!(text.contains("<redacted>"));
        assert_eq!(e.base_url, "https://x/v1");
    }

    #[test]
    fn blank_keys_count_as_none() {
        let e = Endpoint::new(
            "a",
            ProviderKind::LmStudio,
            "http://x/v1",
            Some("  ".into()),
        );
        assert!(!e.has_key());
    }

    #[test]
    fn bad_urls_are_rejected() {
        let e = Endpoint::new("a", ProviderKind::OpenAi, "localhost:1234", None);
        assert!(matches!(connect(e), Err(LlmError::Invalid(_))));
    }
}
