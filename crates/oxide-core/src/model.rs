//! Models offered by the configured endpoints, and what they can do.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::ProviderKind;

/// How a model is asked to think before answering.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ThinkingStyle {
    /// No extended thinking.
    #[default]
    None,
    /// Thinking with an explicit token budget.
    Budget,
    /// The model decides how long to think. It can also be switched off explicitly.
    Adaptive,
}

/// What a model supports. Filled from the provider's own metadata where it has any, and from
/// Oxide's curated tables or a probe otherwise.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ModelCapabilities {
    /// Native function / tool calling. Models without it are hidden by default.
    pub tools: bool,
    pub thinking: ThinkingStyle,
    pub images: bool,
    pub pdfs: bool,
}

/// A model the picker can offer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    /// `<providerId>/<model>`, the form stored on conversations.
    pub id: String,
    pub provider_id: String,
    /// The model's id at the provider.
    pub model: String,
    /// Display name.
    pub label: String,
    pub capabilities: ModelCapabilities,
    pub context_window: Option<u32>,
    pub max_output_tokens: Option<u32>,
}

/// The models of one endpoint, or why they couldn't be listed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderModels {
    pub provider_id: String,
    pub provider_name: String,
    pub provider_kind: ProviderKind,
    pub models: Vec<ModelInfo>,
    /// Set when the endpoint couldn't be reached or rejected the key. Never contains the key.
    pub error: Option<String>,
}

/// Builds the stored model id from an endpoint id and the provider's model id.
pub fn model_id(provider_id: &str, model: &str) -> String {
    format!("{provider_id}/{model}")
}

/// Splits a stored model id into endpoint id and model. Model names may contain `/`
/// (LM Studio uses `publisher/model`); endpoint ids never do.
pub fn split_model_id(id: &str) -> Option<(&str, &str)> {
    let (provider, model) = id.split_once('/')?;
    if provider.is_empty() || model.is_empty() {
        None
    } else {
        Some((provider, model))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_ids_split_at_the_first_slash() {
        assert_eq!(
            split_model_id("lmstudio/qwen/qwen3-14b"),
            Some(("lmstudio", "qwen/qwen3-14b"))
        );
        assert_eq!(split_model_id("anthropic/"), None);
        assert_eq!(split_model_id("nothing"), None);
        assert_eq!(model_id("openai", "gpt-5"), "openai/gpt-5");
    }

    #[test]
    fn thinking_style_serializes_camel_case() {
        let json = serde_json::to_string(&ThinkingStyle::Adaptive).expect("serializes");
        assert_eq!(json, "\"adaptive\"");
    }
}
