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
    /// A local reasoning model whose thinking can only be switched on or off, through the
    /// chat template (`chat_template_kwargs.enable_thinking`). Verified on Strata.
    Switch,
}

/// How much effort a model puts into an answer (thinking, length, tool use). The names match
/// Anthropic's `output_config.effort`; OpenAI-compatible servers get the nearest
/// `reasoning_effort`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Type,
)]
#[serde(rename_all = "camelCase")]
pub enum Effort {
    Low,
    Medium,
    High,
    /// "Extra" in the UI.
    Xhigh,
    Max,
}

impl Effort {
    pub const ALL: [Effort; 5] = [
        Effort::Low,
        Effort::Medium,
        Effort::High,
        Effort::Xhigh,
        Effort::Max,
    ];

    /// The API value.
    pub fn as_str(self) -> &'static str {
        match self {
            Effort::Low => "low",
            Effort::Medium => "medium",
            Effort::High => "high",
            Effort::Xhigh => "xhigh",
            Effort::Max => "max",
        }
    }
}

/// What a model supports. Filled from the provider's own metadata where it has any, and from
/// Oxide's curated tables or a probe otherwise.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ModelCapabilities {
    /// Native function / tool calling. Models without it are hidden by default.
    pub tools: bool,
    pub thinking: ThinkingStyle,
    /// Thinking can be switched off with an explicit `disabled` request. Some models that
    /// always think reject that (Claude Opus 5.5 answers 400), so it's only sent when the
    /// provider says it's supported.
    pub thinking_off: bool,
    pub images: bool,
    pub pdfs: bool,
    /// Effort levels the model accepts, lowest first. Empty: no effort control.
    pub effort: Vec<Effort>,
    /// The level that applies when none is sent.
    pub default_effort: Option<Effort>,
}

impl ModelCapabilities {
    /// The level to use for a requested effort: the request itself if the model accepts it,
    /// otherwise the nearest lower level it accepts (or its lowest). `None` when the model has
    /// no effort control or nothing was requested.
    pub fn effective_effort(&self, requested: Option<Effort>) -> Option<Effort> {
        let requested = requested?;
        if self.effort.contains(&requested) {
            return Some(requested);
        }
        self.effort
            .iter()
            .copied()
            .filter(|e| *e <= requested)
            .max()
            .or_else(|| self.effort.iter().copied().min())
    }
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
    fn effort_names_match_the_api() {
        for e in Effort::ALL {
            let json = serde_json::to_string(&e).expect("serializes");
            assert_eq!(json, format!("\"{}\"", e.as_str()));
        }
        assert!(Effort::Low < Effort::Max);
    }

    #[test]
    fn effort_falls_back_to_the_nearest_lower_level() {
        let caps = ModelCapabilities {
            effort: vec![Effort::Low, Effort::Medium, Effort::High],
            ..ModelCapabilities::default()
        };
        assert_eq!(caps.effective_effort(None), None);
        assert_eq!(
            caps.effective_effort(Some(Effort::Medium)),
            Some(Effort::Medium)
        );
        assert_eq!(caps.effective_effort(Some(Effort::Max)), Some(Effort::High));
        let high_only = ModelCapabilities {
            effort: vec![Effort::High, Effort::Max],
            ..ModelCapabilities::default()
        };
        assert_eq!(
            high_only.effective_effort(Some(Effort::Low)),
            Some(Effort::High)
        );
        assert_eq!(
            ModelCapabilities::default().effective_effort(Some(Effort::Low)),
            None
        );
    }

    #[test]
    fn thinking_style_serializes_camel_case() {
        let json = serde_json::to_string(&ThinkingStyle::Adaptive).expect("serializes");
        assert_eq!(json, "\"adaptive\"");
    }
}
