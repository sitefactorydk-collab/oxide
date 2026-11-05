//! Model provider endpoints. API keys never appear in these types: the store keeps them in
//! the OS keychain and only exposes a masked hint.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::Timestamp;

/// The API family an endpoint speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    Anthropic,
    #[serde(rename = "openai")]
    OpenAi,
    #[serde(rename = "lmstudio")]
    LmStudio,
    Strata,
}

impl ProviderKind {
    pub const ALL: [ProviderKind; 4] = [
        ProviderKind::Anthropic,
        ProviderKind::OpenAi,
        ProviderKind::LmStudio,
        ProviderKind::Strata,
    ];

    /// The value stored in the database (same as the serde name).
    pub fn as_str(self) -> &'static str {
        match self {
            ProviderKind::Anthropic => "anthropic",
            ProviderKind::OpenAi => "openai",
            ProviderKind::LmStudio => "lmstudio",
            ProviderKind::Strata => "strata",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.as_str() == s)
    }

    pub fn label(self) -> &'static str {
        match self {
            ProviderKind::Anthropic => "Anthropic",
            ProviderKind::OpenAi => "OpenAI",
            ProviderKind::LmStudio => "LM Studio",
            ProviderKind::Strata => "Strata",
        }
    }

    /// Default base URL. Every base URL ends at the API version (`…/v1`).
    pub fn default_base_url(self) -> &'static str {
        match self {
            ProviderKind::Anthropic => "https://api.anthropic.com/v1",
            ProviderKind::OpenAi => "https://api.openai.com/v1",
            ProviderKind::LmStudio => "http://127.0.0.1:1234/v1",
            ProviderKind::Strata => "http://127.0.0.1:47831/v1",
        }
    }

    /// Whether the endpoint normally needs an API key (local servers don't).
    pub fn requires_key(self) -> bool {
        matches!(self, ProviderKind::Anthropic | ProviderKind::OpenAi)
    }
}

/// A configured endpoint as the UI sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfig {
    pub id: String,
    pub kind: ProviderKind,
    pub name: String,
    pub base_url: String,
    pub enabled: bool,
    /// Masked form of the stored key (e.g. `…a1b2`), or `None` when no key is stored.
    pub key_hint: Option<String>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// Create (`id: None`) or update an endpoint. Keys are set separately.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInput {
    pub id: Option<String>,
    pub kind: ProviderKind,
    pub name: String,
    pub base_url: String,
    pub enabled: bool,
}

/// Connection test request. Uses `api_key` if given, otherwise the key stored for
/// `provider_id`, so the form can be tested before or after saving.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TestProvider {
    pub kind: ProviderKind,
    pub base_url: String,
    pub api_key: Option<String>,
    pub provider_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderTestResult {
    pub ok: bool,
    /// Human-readable outcome. Never contains the key.
    pub message: String,
    pub model_count: u32,
    /// Up to 20 model ids, as a sample.
    pub models: Vec<String>,
    pub latency_ms: u32,
}

/// Masks an API key for display: the last four characters only.
pub fn mask_key(key: &str) -> String {
    let chars: Vec<char> = key.trim().chars().collect();
    if chars.len() < 12 {
        return "••••".to_owned();
    }
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("••••{tail}")
}

/// Normalizes a base URL: trims whitespace and trailing slashes.
pub fn normalize_base_url(url: &str) -> String {
    url.trim().trim_end_matches('/').to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_round_trip_through_strings_and_serde() {
        for k in ProviderKind::ALL {
            assert_eq!(ProviderKind::parse(k.as_str()), Some(k));
            let json = serde_json::to_string(&k).expect("serializes");
            assert_eq!(json, format!("\"{}\"", k.as_str()));
        }
    }

    #[test]
    fn mask_shows_only_the_tail() {
        assert_eq!(mask_key("sk-ant-api03-abcdefghijkl1234"), "••••1234");
        assert_eq!(mask_key("short"), "••••");
    }

    #[test]
    fn base_urls_are_normalized() {
        assert_eq!(
            normalize_base_url(" http://localhost:1234/v1/ "),
            "http://localhost:1234/v1"
        );
    }

    #[test]
    fn local_kinds_need_no_key() {
        assert!(!ProviderKind::LmStudio.requires_key());
        assert!(ProviderKind::Anthropic.requires_key());
    }
}
