//! MCP sampling: a server asks Oxide's model to answer something (after the user approved
//! it). Converts the `sampling/createMessage` request into a model request and the answer
//! into a `CreateMessageResult`.

use oxide_core::model::split_model_id;
use oxide_core::{ContentBlock, MediaSource, ProviderKind, Role, ThinkingStyle};
use oxide_llm::{ChatMessage, ChatRequest, Thinking, complete_text};
use serde_json::{Value, json};

use crate::{AgentError, ModelRegistry};

/// Upper bound on what a server may ask for.
const MAX_SAMPLING_TOKENS: u64 = 16_000;

fn block(content: &Value) -> Option<ContentBlock> {
    match content.get("type").and_then(Value::as_str)? {
        "text" => Some(ContentBlock::text(content.get("text")?.as_str()?)),
        "image" => Some(ContentBlock::Image {
            media_type: content
                .get("mimeType")
                .and_then(Value::as_str)
                .unwrap_or("image/png")
                .to_owned(),
            source: MediaSource::Base64 {
                data: content.get("data")?.as_str()?.to_owned(),
            },
        }),
        // Audio, tool use and tool results aren't sent to models from sampling.
        _ => None,
    }
}

/// The model request for a `sampling/createMessage` request.
pub fn sampling_request(model: &str, params: &Value) -> Result<ChatRequest, AgentError> {
    let mut messages = Vec::new();
    for m in params
        .get("messages")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let role = match m.get("role").and_then(Value::as_str) {
            Some("assistant") => Role::Assistant,
            _ => Role::User,
        };
        let content: Vec<ContentBlock> = match m.get("content") {
            Some(Value::Array(parts)) => parts.iter().filter_map(block).collect(),
            Some(c) => block(c).into_iter().collect(),
            None => Vec::new(),
        };
        if !content.is_empty() {
            messages.push(ChatMessage { role, content });
        }
    }
    if messages.is_empty() {
        return Err(AgentError::Invalid(
            "the server's sampling request had no messages".into(),
        ));
    }
    let mut request = ChatRequest::new(model, messages);
    request.system = params
        .get("systemPrompt")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let max = params
        .get("maxTokens")
        .and_then(Value::as_u64)
        .unwrap_or(1_024)
        .clamp(16, MAX_SAMPLING_TOKENS);
    request.max_tokens = u32::try_from(max).ok();
    Ok(request)
}

/// Runs an approved sampling request on `model_id` and returns the `CreateMessageResult`.
pub async fn run_sampling(
    registry: &ModelRegistry,
    model_id: &str,
    params: &Value,
) -> Result<Value, AgentError> {
    let (provider_id, model) =
        split_model_id(model_id).ok_or_else(|| AgentError::Invalid("unknown model".into()))?;
    let (config, provider) = registry.provider(provider_id).await?;
    let caps = registry
        .find(model_id)
        .await
        .map(|m| m.capabilities)
        .unwrap_or_default();
    let mut request = sampling_request(model, params)?;
    if config.kind != ProviderKind::Anthropic {
        // Local servers pick their own limit; reasoning models need room to think first.
        request.max_tokens = None;
    }
    if (config.kind == ProviderKind::Anthropic
        && caps.thinking == ThinkingStyle::Adaptive
        && caps.thinking_off)
        || caps.thinking == ThinkingStyle::Switch
    {
        request.thinking = Thinking::Off;
    }
    request.images = caps.images || config.kind == ProviderKind::Strata;
    let text = complete_text(provider.as_ref(), request).await?;
    Ok(json!({
        "role": "assistant",
        "content": {"type": "text", "text": text.trim()},
        "model": model_id,
        "stopReason": "endTurn",
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_are_converted() {
        let r = sampling_request(
            "m",
            &json!({
                "messages": [
                    {"role": "user", "content": {"type": "text", "text": "Hi"}},
                    {"role": "assistant", "content": [{"type": "text", "text": "Yes?"},
                                                         {"type": "audio", "data": "x"}]},
                    {"role": "user", "content": {"type": "image", "data": "AA==", "mimeType": "image/jpeg"}}
                ],
                "systemPrompt": "Be brief.",
                "maxTokens": 999999
            }),
        )
        .expect("request");
        assert_eq!(r.messages.len(), 3);
        assert_eq!(r.messages[1].role, Role::Assistant);
        assert_eq!(r.messages[1].content.len(), 1);
        assert!(
            matches!(&r.messages[2].content[0], ContentBlock::Image { media_type, .. } if media_type == "image/jpeg")
        );
        assert_eq!(r.system.as_deref(), Some("Be brief."));
        assert_eq!(r.max_tokens, Some(16_000));
        assert!(sampling_request("m", &json!({"messages": []})).is_err());
    }
}
