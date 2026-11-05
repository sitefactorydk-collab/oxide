//! Which models support tools, thinking, images and PDFs.
//!
//! - Anthropic: the Models API reports thinking, image and PDF support per model; tool use is
//!   supported by every Claude 3+ model. The curated table below is the fallback when a model
//!   has no capability data.
//! - OpenAI: the Models API reports nothing useful, so a curated table decides by id.
//! - LM Studio: its native `/api/v0/models` endpoint reports `tool_use` and vision.
//! - Strata: nothing is reported; the caller probes each model once.

use oxide_core::{Effort, ModelCapabilities, ThinkingStyle};
use serde_json::Value;

/// The generation number in a Claude model id: `claude-3-5-haiku-…` → 3, `claude-sonnet-4-5` → 4.
fn claude_generation(id: &str) -> Option<u32> {
    id.strip_prefix("claude-")?
        .split('-')
        .find_map(|part| part.parse::<u32>().ok())
}

/// Curated capabilities of a Claude model, from its id alone.
pub fn anthropic_curated(id: &str) -> ModelCapabilities {
    let generation = claude_generation(id).unwrap_or(0);
    let modern = generation >= 3 && !id.contains("instant");
    let point_five_plus = generation >= 4 || id.contains("-3-5") || id.contains("-3-7");
    ModelCapabilities {
        tools: modern,
        thinking: if generation >= 4 || id.contains("-3-7") {
            ThinkingStyle::Budget
        } else {
            ThinkingStyle::None
        },
        thinking_off: false,
        images: modern,
        pdfs: modern && point_five_plus,
        effort: Vec::new(),
        default_effort: None,
    }
}

/// The effort level a Claude model uses when none is sent (from the effort docs: medium on
/// Opus 5.5 and Haiku 5.5, high elsewhere).
pub fn anthropic_default_effort(id: &str) -> Effort {
    if id.starts_with("claude-opus-5-5") || id.starts_with("claude-haiku-5-5") {
        Effort::Medium
    } else {
        Effort::High
    }
}

fn supported(v: Option<&Value>) -> Option<bool> {
    v?.get("supported")?.as_bool()
}

/// Capabilities of a Claude model from a Models API entry, falling back to the curated table
/// for anything the entry doesn't report.
pub fn anthropic_from_api(id: &str, capabilities: Option<&Value>) -> ModelCapabilities {
    let mut caps = anthropic_curated(id);
    let Some(c) = capabilities.filter(|c| c.is_object()) else {
        return caps;
    };
    if let Some(images) = supported(c.get("image_input")) {
        caps.images = images;
    }
    if let Some(pdfs) = supported(c.get("pdf_input")) {
        caps.pdfs = pdfs;
    }
    if let Some(effort) = c.get("effort").filter(|e| supported(Some(e)) == Some(true)) {
        caps.effort = Effort::ALL
            .into_iter()
            .filter(|level| supported(effort.get(level.as_str())) == Some(true))
            .collect();
        if !caps.effort.is_empty() {
            let default = anthropic_default_effort(id);
            caps.default_effort = Some(if caps.effort.contains(&default) {
                default
            } else {
                caps.effort[caps.effort.len() / 2]
            });
        }
    }
    if let Some(thinking) = c.get("thinking") {
        caps.thinking = match supported(Some(thinking)) {
            Some(false) => ThinkingStyle::None,
            Some(true) => {
                let types = thinking.get("types");
                let adaptive = supported(types.and_then(|t| t.get("adaptive")));
                caps.thinking_off = supported(types.and_then(|t| t.get("disabled"))) == Some(true);
                let enabled = supported(types.and_then(|t| t.get("enabled")));
                match (adaptive, enabled) {
                    (Some(true), _) => ThinkingStyle::Adaptive,
                    (_, Some(false)) => ThinkingStyle::None,
                    _ => ThinkingStyle::Budget,
                }
            }
            None => caps.thinking,
        };
    }
    caps
}

/// What an OpenAI model id is, as far as Oxide is concerned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenAiModel {
    /// Not a chat model (embeddings, speech, images, moderation, completions-only). Hidden.
    NotChat,
    Chat(ModelCapabilities),
}

/// Curated table for OpenAI's own API, by model id.
pub fn openai_curated(id: &str) -> OpenAiModel {
    let id = id.strip_prefix("ft:").unwrap_or(id).to_ascii_lowercase();
    const NOT_CHAT: [&str; 17] = [
        "embedding",
        "tts",
        "whisper",
        "dall-e",
        "davinci",
        "babbage",
        "moderation",
        "audio",
        "realtime",
        "transcribe",
        "image",
        "sora",
        "instruct",
        "codex",
        "deep-research",
        "computer-use",
        "omni-",
    ];
    if NOT_CHAT.iter().any(|w| id.contains(w)) {
        return OpenAiModel::NotChat;
    }
    let family = [
        "gpt-3.5-turbo",
        "gpt-4",
        "gpt-5",
        "o1",
        "o3",
        "o4",
        "chatgpt-",
    ];
    if !family.iter().any(|f| id.starts_with(f)) {
        return OpenAiModel::NotChat;
    }
    // Chat models that can't call functions.
    let no_tools = id.starts_with("chatgpt-")
        || id.starts_with("o1-mini")
        || id.starts_with("o1-preview")
        || id.contains("search")
        || id.contains("chat-latest");
    let images = id.starts_with("gpt-4o")
        || id.starts_with("gpt-4.1")
        || id.starts_with("gpt-4-turbo")
        || id.starts_with("gpt-4.5")
        || id.starts_with("gpt-5")
        || (id.starts_with("o1") && !id.starts_with("o1-mini") && !id.starts_with("o1-preview"))
        || id.starts_with("o3")
        || id.starts_with("o4");
    // Reasoning models take `reasoning_effort` (low / medium / high).
    let reasoning =
        (id.starts_with("o1") && !id.starts_with("o1-mini") && !id.starts_with("o1-preview"))
            || id.starts_with("o3")
            || id.starts_with("o4")
            || (id.starts_with("gpt-5") && !id.contains("chat"));
    let (effort, default_effort) = if reasoning {
        (
            vec![Effort::Low, Effort::Medium, Effort::High],
            Some(Effort::Medium),
        )
    } else {
        (Vec::new(), None)
    };
    OpenAiModel::Chat(ModelCapabilities {
        tools: !no_tools,
        thinking: ThinkingStyle::None,
        thinking_off: false,
        images,
        pdfs: false,
        effort,
        default_effort,
    })
}

/// Capabilities from an LM Studio `/api/v0/models` entry. `None` for non-chat models.
pub fn lmstudio_from_native(entry: &Value) -> Option<ModelCapabilities> {
    let kind = entry.get("type").and_then(Value::as_str).unwrap_or("llm");
    if kind == "embeddings" || kind == "embedding" {
        return None;
    }
    let caps = entry.get("capabilities");
    let has = |name: &str| match caps {
        Some(Value::Array(list)) => list.iter().any(|c| c.as_str() == Some(name)),
        Some(Value::Object(map)) => map.get(name).and_then(Value::as_bool).unwrap_or(false),
        _ => false,
    };
    Some(ModelCapabilities {
        tools: has("tool_use") || has("trained_for_tool_use"),
        images: kind == "vlm" || has("vision"),
        ..ModelCapabilities::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn claude_generations_parse() {
        assert_eq!(claude_generation("claude-3-5-haiku-20241022"), Some(3));
        assert_eq!(claude_generation("claude-sonnet-4-5-20250929"), Some(4));
        assert_eq!(claude_generation("claude-opus-5-5"), Some(5));
        assert_eq!(claude_generation("gpt-4o"), None);
    }

    #[test]
    fn curated_claude_table() {
        let old = anthropic_curated("claude-2.1");
        assert!(!old.tools);
        let haiku3 = anthropic_curated("claude-3-haiku-20240307");
        assert!(haiku3.tools && haiku3.images && !haiku3.pdfs);
        assert_eq!(haiku3.thinking, ThinkingStyle::None);
        let s37 = anthropic_curated("claude-3-7-sonnet-20250219");
        assert_eq!(s37.thinking, ThinkingStyle::Budget);
        assert!(s37.pdfs);
        let new = anthropic_curated("claude-fable-5-1");
        assert!(new.tools && new.pdfs);
        assert_eq!(new.thinking, ThinkingStyle::Budget);
    }

    #[test]
    fn api_capabilities_override_the_table() {
        let caps = json!({
            "image_input": {"supported": true},
            "pdf_input": {"supported": false},
            "thinking": {"supported": true, "types": {
                "adaptive": {"supported": true}, "enabled": {"supported": true}
            }}
        });
        let c = anthropic_from_api("claude-opus-5-5", Some(&caps));
        assert_eq!(c.thinking, ThinkingStyle::Adaptive);
        // `disabled` not reported as supported: never send it.
        assert!(!c.thinking_off);
        let switchable = json!({"thinking": {"supported": true, "types": {
            "adaptive": {"supported": true}, "disabled": {"supported": true}
        }}});
        assert!(anthropic_from_api("claude-haiku-5-5", Some(&switchable)).thinking_off);
        assert!(!c.pdfs);
        assert!(c.tools);

        let budget_only = json!({"thinking": {"supported": true, "types": {
            "adaptive": {"supported": false}, "enabled": {"supported": true}
        }}});
        let c = anthropic_from_api("claude-sonnet-4-5", Some(&budget_only));
        assert_eq!(c.thinking, ThinkingStyle::Budget);

        let none = json!({"thinking": {"supported": false}});
        let c = anthropic_from_api("claude-sonnet-4-5", Some(&none));
        assert_eq!(c.thinking, ThinkingStyle::None);

        // Effort levels come from the API; the default follows the docs.
        let effort = json!({"effort": {"supported": true,
            "low": {"supported": true}, "medium": {"supported": true}, "high": {"supported": true},
            "max": {"supported": true}, "xhigh": null}});
        let c = anthropic_from_api("claude-sonnet-4-6", Some(&effort));
        assert_eq!(
            c.effort,
            vec![Effort::Low, Effort::Medium, Effort::High, Effort::Max]
        );
        assert_eq!(c.default_effort, Some(Effort::High));
        let c = anthropic_from_api("claude-opus-5-5", Some(&effort));
        assert_eq!(c.default_effort, Some(Effort::Medium));
        let none = json!({"effort": {"supported": false, "low": {"supported": false}}});
        assert!(
            anthropic_from_api("claude-sonnet-4-5", Some(&none))
                .effort
                .is_empty()
        );

        // Null capabilities fall back to the table.
        let c = anthropic_from_api("claude-3-7-sonnet", Some(&Value::Null));
        assert_eq!(c.thinking, ThinkingStyle::Budget);
    }

    #[test]
    fn curated_openai_table() {
        assert_eq!(
            openai_curated("text-embedding-3-large"),
            OpenAiModel::NotChat
        );
        assert_eq!(openai_curated("gpt-4o-mini-tts"), OpenAiModel::NotChat);
        assert_eq!(openai_curated("gpt-image-1"), OpenAiModel::NotChat);
        assert_eq!(
            openai_curated("gpt-3.5-turbo-instruct"),
            OpenAiModel::NotChat
        );
        assert_eq!(openai_curated("some-other-thing"), OpenAiModel::NotChat);
        let OpenAiModel::Chat(c) = openai_curated("gpt-4.1-mini") else {
            panic!("chat model")
        };
        assert!(c.tools && c.images);
        let OpenAiModel::Chat(c) = openai_curated("chatgpt-4o-latest") else {
            panic!("chat model")
        };
        assert!(!c.tools);
        let OpenAiModel::Chat(c) = openai_curated("o1-mini") else {
            panic!("chat model")
        };
        assert!(!c.tools);
        let OpenAiModel::Chat(c) = openai_curated("ft:gpt-4o-2024-08-06:org::abc") else {
            panic!("chat model")
        };
        assert!(c.tools);
        let OpenAiModel::Chat(c) = openai_curated("gpt-3.5-turbo") else {
            panic!("chat model")
        };
        assert!(c.tools && !c.images && c.effort.is_empty());
        let OpenAiModel::Chat(c) = openai_curated("o4-mini") else {
            panic!("chat model")
        };
        assert_eq!(c.effort.len(), 3);
        assert_eq!(c.default_effort, Some(Effort::Medium));
    }

    #[test]
    fn lmstudio_native_entries() {
        let tool = json!({"id": "qwen/qwen3-14b", "type": "llm", "capabilities": ["tool_use"]});
        assert!(lmstudio_from_native(&tool).is_some_and(|c| c.tools && !c.images));
        let vlm = json!({"id": "gemma", "type": "vlm"});
        assert!(lmstudio_from_native(&vlm).is_some_and(|c| !c.tools && c.images));
        let obj =
            json!({"type": "llm", "capabilities": {"trained_for_tool_use": true, "vision": true}});
        assert!(lmstudio_from_native(&obj).is_some_and(|c| c.tools && c.images));
        let emb = json!({"id": "nomic", "type": "embeddings"});
        assert_eq!(lmstudio_from_native(&emb), None);
    }
}
