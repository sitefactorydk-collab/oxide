//! The models every enabled endpoint offers, with their capabilities.
//!
//! Listings are cached in memory for a few minutes (the picker opens often). Endpoints that
//! don't report tool support (Strata) are probed once per model; the result is kept in the
//! database until the user refreshes in Settings → Models.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures_util::future::join_all;
use oxide_core::model::split_model_id;
use oxide_core::{ModelInfo, ProviderConfig, ProviderModels};
use oxide_llm::{Endpoint, LlmError, Provider};
use oxide_store::{SecretStore, Store};

use crate::AgentError;

/// How long a model listing is reused.
const LISTING_TTL: Duration = Duration::from_secs(10 * 60);

/// Creates the client for an endpoint. Tests swap in fakes.
pub type Connector = Arc<dyn Fn(Endpoint) -> Result<Arc<dyn Provider>, LlmError> + Send + Sync>;

struct Cached {
    at: Instant,
    listing: ProviderModels,
}

pub struct ModelRegistry {
    store: Store,
    secrets: Arc<dyn SecretStore>,
    connector: Connector,
    cache: Mutex<HashMap<String, Cached>>,
}

impl ModelRegistry {
    pub fn new(store: Store, secrets: Arc<dyn SecretStore>) -> Self {
        Self::with_connector(store, secrets, Arc::new(oxide_llm::connect))
    }

    pub fn with_connector(
        store: Store,
        secrets: Arc<dyn SecretStore>,
        connector: Connector,
    ) -> Self {
        Self {
            store,
            secrets,
            connector,
            cache: Mutex::new(HashMap::new()),
        }
    }

    fn cache(&self) -> std::sync::MutexGuard<'_, HashMap<String, Cached>> {
        // A panic while holding the lock can't leave the map half-updated; keep using it.
        self.cache.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Forgets cached listings (after an endpoint, its key or its URL changed).
    pub fn invalidate(&self) {
        self.cache().clear();
    }

    /// The client for one endpoint, with its stored key.
    pub async fn provider(
        &self,
        provider_id: &str,
    ) -> Result<(ProviderConfig, Arc<dyn Provider>), AgentError> {
        let config = self.store.get_provider(provider_id).await?;
        let provider = self.connect(&config).await?;
        Ok((config, provider))
    }

    async fn connect(&self, config: &ProviderConfig) -> Result<Arc<dyn Provider>, AgentError> {
        let key = self.store.provider_key(&self.secrets, &config.id).await?;
        let endpoint = Endpoint::new(
            config.id.clone(),
            config.kind,
            &config.base_url,
            key.map(|k| k.expose().to_owned()),
        );
        Ok((self.connector)(endpoint)?)
    }

    /// Models of every enabled endpoint, in the endpoints' order. `refresh` skips the cache
    /// and probes unverified models again.
    pub async fn list(&self, refresh: bool) -> Result<Vec<ProviderModels>, AgentError> {
        let configs: Vec<ProviderConfig> = self
            .store
            .list_providers()
            .await?
            .into_iter()
            .filter(|p| p.enabled)
            .collect();
        if refresh {
            self.invalidate();
        }
        let listings = join_all(configs.iter().map(|c| self.listing(c, refresh))).await;
        Ok(listings)
    }

    /// One endpoint's models, from the cache when fresh.
    async fn listing(&self, config: &ProviderConfig, reprobe: bool) -> ProviderModels {
        if let Some(hit) = self.cache().get(&config.id)
            && hit.at.elapsed() < LISTING_TTL
        {
            return hit.listing.clone();
        }
        let listing = self.fetch(config, reprobe).await;
        // Failed listings aren't cached, so a server that comes up is seen next time.
        if listing.error.is_none() {
            self.cache().insert(
                config.id.clone(),
                Cached {
                    at: Instant::now(),
                    listing: listing.clone(),
                },
            );
        }
        listing
    }

    async fn fetch(&self, config: &ProviderConfig, reprobe: bool) -> ProviderModels {
        let mut out = ProviderModels {
            provider_id: config.id.clone(),
            provider_name: config.name.clone(),
            provider_kind: config.kind,
            models: Vec::new(),
            error: None,
        };
        if config.kind.requires_key() && config.key_hint.is_none() {
            out.error = Some("No API key yet. Add one in Settings → Providers.".into());
            return out;
        }
        let provider = match self.connect(config).await {
            Ok(p) => p,
            Err(e) => {
                out.error = Some(e.to_string());
                return out;
            }
        };
        let listing = match provider.list_models().await {
            Ok(l) => l,
            Err(e) => {
                out.error = Some(e.to_string());
                return out;
            }
        };
        let mut models = listing.models;
        if !listing.unverified.is_empty() {
            if reprobe && let Err(e) = self.store.clear_probe_results(&config.base_url).await {
                tracing::warn!("couldn't clear probe results: {e}");
            }
            // One at a time: each probe may load a model into a local server's memory.
            for name in &listing.unverified {
                let tools = match self.store.probe_result(&config.base_url, name).await {
                    Ok(Some(known)) => Some(known),
                    _ => match provider.probe_tools(name).await {
                        Ok(tools) => {
                            if let Err(e) = self
                                .store
                                .save_probe_result(&config.base_url, name, tools)
                                .await
                            {
                                tracing::warn!("couldn't save probe result: {e}");
                            }
                            Some(tools)
                        }
                        Err(e) => {
                            tracing::info!(provider = %config.id, model = %name, "tool probe failed: {e}");
                            out.error.get_or_insert_with(|| {
                                format!("Couldn't check tool support for every model: {e}")
                            });
                            None
                        }
                    },
                };
                if let Some(tools) = tools
                    && let Some(m) = models.iter_mut().find(|m| &m.model == name)
                {
                    m.capabilities.tools = tools;
                }
            }
        }
        out.models = models;
        out
    }

    /// Details of a stored model id (`<providerId>/<model>`), listing its endpoint if needed.
    /// `None` when the endpoint can't list it (the chat still tries to use it).
    pub async fn find(&self, model_id: &str) -> Option<ModelInfo> {
        let (provider_id, _) = split_model_id(model_id)?;
        let config = self.store.get_provider(provider_id).await.ok()?;
        let listing = self.listing(&config, false).await;
        listing.models.into_iter().find(|m| m.id == model_id)
    }

    /// Models of one endpoint (cached), for picking a title model.
    pub async fn models_of(&self, provider_id: &str) -> Vec<ModelInfo> {
        match self.store.get_provider(provider_id).await {
            Ok(config) => self.listing(&config, false).await.models,
            Err(_) => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::tests::{FakeProvider, fake_connector};
    use oxide_core::{ModelCapabilities, ProviderInput, ProviderKind};
    use oxide_store::{MemorySecrets, Secret};
    use std::sync::atomic::Ordering;

    async fn setup() -> (Store, Arc<dyn SecretStore>) {
        let store = Store::open_in_memory().await.expect("store");
        let secrets: Arc<dyn SecretStore> = Arc::new(MemorySecrets::default());
        (store, secrets)
    }

    #[tokio::test]
    async fn endpoints_without_keys_report_it_without_requests() {
        let (store, secrets) = setup().await;
        let fake = FakeProvider::new(vec![]);
        let reg = ModelRegistry::with_connector(store, secrets, fake_connector(fake.clone()));
        let lists = reg.list(false).await.expect("lists");
        assert_eq!(lists.len(), 4);
        let anthropic = &lists[0];
        assert_eq!(anthropic.provider_kind, ProviderKind::Anthropic);
        assert!(
            anthropic
                .error
                .as_deref()
                .is_some_and(|e| e.contains("No API key"))
        );
        // Local endpoints were asked (the fake serves one model to each).
        assert_eq!(lists[2].models.len(), 1);
        assert_eq!(fake.list_calls.load(Ordering::SeqCst), 2);
        // Cached the second time.
        reg.list(false).await.expect("lists");
        assert_eq!(fake.list_calls.load(Ordering::SeqCst), 2);
        reg.list(true).await.expect("lists");
        assert_eq!(fake.list_calls.load(Ordering::SeqCst), 4);
    }

    #[tokio::test]
    async fn disabled_endpoints_are_left_out() {
        let (store, secrets) = setup().await;
        store
            .save_provider(ProviderInput {
                id: Some("openai".into()),
                kind: ProviderKind::OpenAi,
                name: "OpenAI".into(),
                base_url: "https://api.openai.com/v1".into(),
                enabled: false,
            })
            .await
            .expect("saves");
        let reg = ModelRegistry::with_connector(
            store,
            secrets,
            fake_connector(FakeProvider::new(vec![])),
        );
        let lists = reg.list(false).await.expect("lists");
        assert!(lists.iter().all(|l| l.provider_id != "openai"));
    }

    #[tokio::test]
    async fn unverified_models_are_probed_once_and_remembered() {
        let (store, secrets) = setup().await;
        store
            .set_provider_key(&secrets, "anthropic", Secret::new("sk-ant-xxxxxxxxxxxx"))
            .await
            .expect("key");
        let fake = FakeProvider::new(vec![])
            .unverified(true)
            .probe_answer(true);
        let reg = ModelRegistry::with_connector(store, secrets, fake_connector(fake.clone()));
        let lists = reg.list(false).await.expect("lists");
        // Every endpoint is the same fake here; each probes its one model once.
        assert!(
            lists
                .iter()
                .all(|l| l.models.iter().all(|m| m.capabilities.tools))
        );
        let probes = fake.probe_calls.load(Ordering::SeqCst);
        assert_eq!(probes, 3);
        reg.invalidate();
        reg.list(false).await.expect("lists");
        assert_eq!(
            fake.probe_calls.load(Ordering::SeqCst),
            probes,
            "remembered in the store"
        );
        reg.list(true).await.expect("lists");
        assert_eq!(
            fake.probe_calls.load(Ordering::SeqCst),
            probes * 2,
            "refresh probes again"
        );
    }

    #[tokio::test]
    async fn find_returns_the_listed_model() {
        let (store, secrets) = setup().await;
        let reg = ModelRegistry::with_connector(
            store,
            secrets,
            fake_connector(FakeProvider::new(vec![])),
        );
        let m = reg.find("lmstudio/fake-model").await.expect("found");
        assert_eq!(
            m.capabilities,
            ModelCapabilities {
                tools: true,
                ..ModelCapabilities::default()
            }
        );
        assert!(reg.find("lmstudio/other").await.is_none());
        assert!(reg.find("nope/fake-model").await.is_none());
    }
}
