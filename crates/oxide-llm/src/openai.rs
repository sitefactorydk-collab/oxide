//! OpenAI-compatible Chat Completions: OpenAI itself, LM Studio and Strata.
//!
//! Streaming with tool calls (including several in parallel), images as data URLs, and the
//! `reasoning_content` deltas some local servers send for reasoning models.

use std::collections::BTreeMap;
use std::time::Duration;

use futures_util::FutureExt;
use futures_util::future::BoxFuture;
use oxide_core::{
    ContentBlock, ModelCapabilities, ModelInfo, ProviderKind, Role, StopReason, ThinkingStyle,
    Usage, model::model_id,
};
use serde_json::{Map, Value, json};

use crate::capabilities::{OpenAiModel, lmstudio_from_native, openai_curated};
use crate::http::{describe, send_with_retry, status_error, streaming_client};
use crate::media::{IMAGE_OMITTED, base64_of, is_text, pdf_as_text, text_of};
use crate::sse::SseEvent;
use crate::stream::{Decoder, Out, sse_stream};
use crate::{
    ChatEvent, ChatRequest, ChatStream, Endpoint, LlmError, ModelListing, Provider, Thinking,
};

const LIST_TIMEOUT: Duration = Duration::from_secs(20);
/// A probe may have to wait for a local server to load the model.
const PROBE_TIMEOUT: Duration = Duration::from_secs(120);

pub struct OpenAiCompatProvider {
    endpoint: Endpoint,
    client: reqwest::Client,
}

impl OpenAiCompatProvider {
    pub fn new(endpoint: Endpoint) -> Result<Self, LlmError> {
        Ok(Self {
            endpoint,
            client: streaming_client()?,
        })
    }

    fn auth(&self, req: reqwest::RequestBuilder) -> Result<reqwest::RequestBuilder, LlmError> {
        match (self.endpoint.api_key(), self.endpoint.kind) {
            (Some(k), _) => Ok(req.bearer_auth(k)),
            (None, ProviderKind::OpenAi) => Err(LlmError::MissingKey),
            (None, _) => Ok(req),
        }
    }

    async fn get_json(&self, url: &str) -> Result<Value, LlmError> {
        let req = self.auth(self.client.get(url))?.timeout(LIST_TIMEOUT);
        let res = send_with_retry(req, &self.endpoint.base_url).await?;
        res.json()
            .await
            .map_err(|e| LlmError::Decode(format!("model list: {}", describe(&e))))
    }

    /// Ids from an OpenAI-style `GET /models`.
    async fn model_ids(&self) -> Result<Vec<String>, LlmError> {
        let v = self
            .get_json(&format!("{}/models", self.endpoint.base_url))
            .await?;
        let data = v
            .get("data")
            .and_then(Value::as_array)
            .ok_or_else(|| LlmError::Decode("no model list in the response".into()))?;
        Ok(data
            .iter()
            .filter_map(|m| m.get("id").and_then(Value::as_str).map(str::to_owned))
            .collect())
    }

    fn info(
        &self,
        model: &str,
        capabilities: ModelCapabilities,
        context: Option<u32>,
    ) -> ModelInfo {
        ModelInfo {
            id: model_id(&self.endpoint.id, model),
            provider_id: self.endpoint.id.clone(),
            model: model.to_owned(),
            label: model.to_owned(),
            capabilities,
            context_window: context,
            max_output_tokens: None,
        }
    }

    async fn models(&self) -> Result<ModelListing, LlmError> {
        match self.endpoint.kind {
            ProviderKind::OpenAi => {
                let mut ids = self.model_ids().await?;
                ids.sort();
                let models = ids
                    .iter()
                    .filter_map(|id| match openai_curated(id) {
                        OpenAiModel::Chat(caps) => Some(self.info(id, caps, None)),
                        OpenAiModel::NotChat => None,
                    })
                    .collect();
                Ok(ModelListing {
                    models,
                    unverified: Vec::new(),
                })
            }
            ProviderKind::LmStudio => match self.lmstudio_native().await {
                Ok(listing) => Ok(listing),
                Err(e) => {
                    // Older LM Studio without the native API: list models, tool support unknown.
                    tracing::info!(
                        "LM Studio native model list unavailable ({e}); using /v1/models"
                    );
                    let models = self
                        .model_ids()
                        .await?
                        .iter()
                        .filter(|id| !id.contains("embed"))
                        .map(|id| self.info(id, ModelCapabilities::default(), None))
                        .collect();
                    Ok(ModelListing {
                        models,
                        unverified: Vec::new(),
                    })
                }
            },
            ProviderKind::Strata | ProviderKind::Anthropic => {
                let ids: Vec<String> = self
                    .model_ids()
                    .await?
                    .into_iter()
                    .filter(|id| !id.contains("embed"))
                    .collect();
                // Strata honors `chat_template_kwargs.enable_thinking` (tested in Phase 4);
                // models without a thinking mode ignore it.
                let caps = ModelCapabilities {
                    thinking: ThinkingStyle::Switch,
                    thinking_off: true,
                    ..ModelCapabilities::default()
                };
                let models = ids
                    .iter()
                    .map(|id| self.info(id, caps.clone(), None))
                    .collect();
                Ok(ModelListing {
                    models,
                    unverified: ids,
                })
            }
        }
    }

    /// LM Studio's own model list (`/api/v0/models`), which reports tool support.
    async fn lmstudio_native(&self) -> Result<ModelListing, LlmError> {
        let base = &self.endpoint.base_url;
        let root = base.strip_suffix("/v1").unwrap_or(base);
        let v = self.get_json(&format!("{root}/api/v0/models")).await?;
        let data = v
            .get("data")
            .and_then(Value::as_array)
            .ok_or_else(|| LlmError::Decode("no model list in the response".into()))?;
        let models = data
            .iter()
            .filter_map(|entry| {
                let id = entry.get("id").and_then(Value::as_str)?;
                let caps = lmstudio_from_native(entry)?;
                let context = entry
                    .get("max_context_length")
                    .and_then(Value::as_u64)
                    .and_then(|n| u32::try_from(n).ok());
                Some(self.info(id, caps, context))
            })
            .collect();
        Ok(ModelListing {
            models,
            unverified: Vec::new(),
        })
    }

    async fn start(&self, request: ChatRequest) -> Result<ChatStream, LlmError> {
        let body = request_body(self.endpoint.kind, &request).await?;
        let url = format!("{}/chat/completions", self.endpoint.base_url);
        let req = self
            .auth(self.client.post(&url))?
            .header("accept", "text/event-stream")
            .json(&body);
        let res = send_with_retry(req, &self.endpoint.base_url).await?;
        Ok(sse_stream(res, OpenAiDecoder::default()))
    }

    async fn probe(&self, model: &str) -> Result<bool, LlmError> {
        let body = json!({
            "model": model,
            "messages": [{"role": "user", "content": "Call the ping tool now."}],
            "tools": [{
                "type": "function",
                "function": {
                    "name": "ping",
                    "description": "Replies with pong. Call it when asked to ping.",
                    "parameters": {"type": "object", "properties": {}},
                },
            }],
            "max_tokens": 256,
            "stream": false,
        });
        let url = format!("{}/chat/completions", self.endpoint.base_url);
        let req = self
            .auth(self.client.post(&url))?
            .timeout(PROBE_TIMEOUT)
            .json(&body);
        let res = req.send().await.map_err(|e| LlmError::Unreachable {
            url: self.endpoint.base_url.clone(),
            reason: describe(&e),
        })?;
        let status = res.status().as_u16();
        let text = res.text().await.unwrap_or_default();
        if status == 400 || status == 422 {
            // The server rejected the tools parameter for this model.
            return Ok(false);
        }
        if !(200..300).contains(&status) {
            return Err(status_error(status, &text));
        }
        let v: Value = serde_json::from_str(&text)
            .map_err(|e| LlmError::Decode(format!("probe reply: {e}")))?;
        Ok(v.pointer("/choices/0/message/tool_calls")
            .and_then(Value::as_array)
            .is_some_and(|calls| !calls.is_empty()))
    }
}

impl Provider for OpenAiCompatProvider {
    fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    fn list_models(&self) -> BoxFuture<'_, Result<ModelListing, LlmError>> {
        self.models().boxed()
    }

    fn stream(&self, request: ChatRequest) -> BoxFuture<'_, Result<ChatStream, LlmError>> {
        self.start(request).boxed()
    }

    fn probe_tools<'a>(&'a self, model: &'a str) -> BoxFuture<'a, Result<bool, LlmError>> {
        self.probe(model).boxed()
    }
}

/// Builds the Chat Completions request body.
pub(crate) async fn request_body(kind: ProviderKind, req: &ChatRequest) -> Result<Value, LlmError> {
    let mut messages: Vec<Value> = Vec::new();
    if let Some(system) = req.system.as_deref().filter(|s| !s.trim().is_empty()) {
        messages.push(json!({ "role": "system", "content": system }));
    }
    for m in &req.messages {
        match m.role {
            Role::User => user_messages(&m.content, req, &mut messages).await?,
            Role::Assistant => {
                if let Some(msg) = assistant_message(&m.content) {
                    messages.push(msg);
                }
            }
        }
    }

    let mut body = Map::new();
    body.insert("model".into(), json!(req.model));
    body.insert("messages".into(), json!(messages));
    body.insert("stream".into(), json!(true));
    if matches!(kind, ProviderKind::OpenAi | ProviderKind::LmStudio) {
        body.insert("stream_options".into(), json!({ "include_usage": true }));
    }
    if let Some(max) = req.max_tokens {
        let key = if kind == ProviderKind::OpenAi {
            "max_completion_tokens"
        } else {
            "max_tokens"
        };
        body.insert(key.into(), json!(max));
    }
    if let Some(effort) = req.effort
        && kind == ProviderKind::OpenAi
    {
        // OpenAI reasoning models take low / medium / high.
        let level = match effort {
            oxide_core::Effort::Low => "low",
            oxide_core::Effort::Medium => "medium",
            _ => "high",
        };
        body.insert("reasoning_effort".into(), json!(level));
    }
    if req.thinking == Thinking::Off && kind != ProviderKind::OpenAi {
        body.insert(
            "chat_template_kwargs".into(),
            json!({ "enable_thinking": false }),
        );
    }
    if !req.tools.is_empty() {
        let tools: Vec<Value> = req
            .tools
            .iter()
            .map(|t| {
                json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.input_schema,
                    },
                })
            })
            .collect();
        body.insert("tools".into(), json!(tools));
    }
    Ok(Value::Object(body))
}

/// A user turn: tool results become `tool` messages first, then the rest as one user message.
async fn user_messages(
    content: &[ContentBlock],
    req: &ChatRequest,
    out: &mut Vec<Value>,
) -> Result<(), LlmError> {
    let mut parts: Vec<Value> = Vec::new();
    let mut has_media = false;
    for block in content {
        match block {
            ContentBlock::ToolResult {
                tool_use_id,
                content,
                is_error,
            } => {
                let mut text = oxide_core::message::plain_text(content);
                if *is_error {
                    text = format!("Error: {text}");
                }
                out.push(json!({ "role": "tool", "tool_call_id": tool_use_id, "content": text }));
            }
            ContentBlock::Text { text } if !text.trim().is_empty() => {
                parts.push(json!({ "type": "text", "text": text }));
            }
            ContentBlock::Image { media_type, source } => {
                if req.images {
                    has_media = true;
                    let url = format!("data:{media_type};base64,{}", base64_of(source).await?);
                    parts.push(json!({ "type": "image_url", "image_url": { "url": url } }));
                } else {
                    parts.push(json!({ "type": "text", "text": IMAGE_OMITTED }));
                }
            }
            ContentBlock::Document {
                media_type,
                name,
                source,
            } => {
                let text = if is_text(media_type) {
                    format!(
                        "<file name=\"{name}\">\n{}\n</file>",
                        text_of(source).await?
                    )
                } else if media_type == "application/pdf" {
                    pdf_as_text(name, source).await
                } else {
                    format!("[Attached file {name} ({media_type}) can't be read by this model.]")
                };
                parts.push(json!({ "type": "text", "text": text }));
            }
            _ => {}
        }
    }
    if parts.is_empty() {
        return Ok(());
    }
    let content = if has_media {
        json!(parts)
    } else {
        let texts: Vec<&str> = parts.iter().filter_map(|p| p["text"].as_str()).collect();
        json!(texts.join("\n\n"))
    };
    out.push(json!({ "role": "user", "content": content }));
    Ok(())
}

fn assistant_message(content: &[ContentBlock]) -> Option<Value> {
    let text: Vec<&str> = content
        .iter()
        .filter_map(|b| match b {
            ContentBlock::Text { text } if !text.trim().is_empty() => Some(text.as_str()),
            _ => None,
        })
        .collect();
    let calls: Vec<Value> = content
        .iter()
        .filter_map(|b| match b {
            ContentBlock::ToolUse {
                id,
                name,
                input_json,
            } => Some(json!({
                "id": id,
                "type": "function",
                "function": { "name": name, "arguments": input_json },
            })),
            _ => None,
        })
        .collect();
    if text.is_empty() && calls.is_empty() {
        return None;
    }
    let mut msg = json!({
        "role": "assistant",
        "content": if text.is_empty() { Value::Null } else { json!(text.join("\n\n")) },
    });
    if !calls.is_empty() {
        msg["tool_calls"] = json!(calls);
    }
    Some(msg)
}

fn finish_reason(s: &str) -> StopReason {
    match s {
        "stop" => StopReason::EndTurn,
        "length" => StopReason::MaxTokens,
        "tool_calls" | "function_call" => StopReason::ToolUse,
        "content_filter" => StopReason::Refusal,
        _ => StopReason::Other,
    }
}

/// Decodes Chat Completions stream chunks.
#[derive(Default)]
pub(crate) struct OpenAiDecoder {
    /// Tool call index → (id, ended).
    tools: BTreeMap<u64, String>,
    stop: Option<StopReason>,
    finished: bool,
}

impl OpenAiDecoder {
    fn finish(&mut self, out: &mut Out) {
        for (_, id) in std::mem::take(&mut self.tools) {
            out.push_back(Ok(ChatEvent::ToolUseEnd { id }));
        }
        self.finished = true;
        out.push_back(Ok(ChatEvent::Stop(
            self.stop.unwrap_or(StopReason::EndTurn),
        )));
    }
}

impl Decoder for OpenAiDecoder {
    fn event(&mut self, event: SseEvent, out: &mut Out) -> bool {
        let data = event.data.trim();
        if data == "[DONE]" {
            self.finish(out);
            return true;
        }
        let Ok(v) = serde_json::from_str::<Value>(data) else {
            return false;
        };
        if let Some(err) = v.get("error").filter(|e| !e.is_null()) {
            let message = err
                .get("message")
                .and_then(Value::as_str)
                .or(err.as_str())
                .unwrap_or("unknown error");
            self.finished = true;
            out.push_back(Err(LlmError::Stream(format!(
                "the server reported an error: {message}"
            ))));
            return true;
        }
        if let Some(u) = v.get("usage").filter(|u| u.is_object()) {
            let n = |p: &str| {
                u.pointer(p)
                    .and_then(Value::as_u64)
                    .map(|x| u32::try_from(x).unwrap_or(u32::MAX))
                    .unwrap_or(0)
            };
            out.push_back(Ok(ChatEvent::Usage(Usage {
                input_tokens: n("/prompt_tokens"),
                output_tokens: n("/completion_tokens"),
                cache_read_tokens: n("/prompt_tokens_details/cached_tokens"),
                cache_write_tokens: 0,
            })));
        }
        let Some(choice) = v
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|c| c.first())
        else {
            return false;
        };
        let delta = &choice["delta"];
        for key in ["reasoning_content", "reasoning"] {
            if let Some(t) = delta[key].as_str().filter(|t| !t.is_empty()) {
                out.push_back(Ok(ChatEvent::ThinkingDelta(t.to_owned())));
                break;
            }
        }
        if let Some(t) = delta["content"].as_str().filter(|t| !t.is_empty()) {
            out.push_back(Ok(ChatEvent::TextDelta(t.to_owned())));
        }
        if let Some(calls) = delta["tool_calls"].as_array() {
            for (pos, call) in calls.iter().enumerate() {
                let index = call["index"].as_u64().unwrap_or(pos as u64);
                let id = match self.tools.get(&index) {
                    Some(id) => id.clone(),
                    None => {
                        let id = call["id"]
                            .as_str()
                            .filter(|s| !s.is_empty())
                            .map_or_else(|| format!("call_{index}"), str::to_owned);
                        let name = call
                            .pointer("/function/name")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_owned();
                        self.tools.insert(index, id.clone());
                        out.push_back(Ok(ChatEvent::ToolUseStart {
                            id: id.clone(),
                            name,
                        }));
                        id
                    }
                };
                if let Some(args) = call
                    .pointer("/function/arguments")
                    .and_then(Value::as_str)
                    .filter(|a| !a.is_empty())
                {
                    out.push_back(Ok(ChatEvent::ToolUseDelta {
                        id,
                        partial_json: args.to_owned(),
                    }));
                }
            }
        }
        if let Some(r) = choice["finish_reason"].as_str() {
            self.stop = Some(finish_reason(r));
        }
        false
    }

    fn eof(&mut self, out: &mut Out) {
        if self.finished {
            return;
        }
        if self.stop.is_some() {
            // Some servers end after the finish reason without sending [DONE].
            self.finish(out);
        } else {
            self.finished = true;
            out.push_back(Err(LlmError::Stream(
                "the connection closed before the reply finished".into(),
            )));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_server::{Reply, serve};
    use crate::{ChatMessage, ToolSpec, connect};
    use futures_util::StreamExt;
    use oxide_core::MediaSource;

    fn endpoint(kind: ProviderKind, base: &str, key: Option<&str>) -> Endpoint {
        Endpoint::new(kind.as_str(), kind, base, key.map(str::to_owned))
    }

    fn chunk(v: Value) -> String {
        format!("data: {v}\n\n")
    }

    #[tokio::test]
    async fn streams_text_reasoning_parallel_tool_calls_and_usage() {
        let body = [
            chunk(json!({"choices":[{"index":0,"delta":{"role":"assistant","reasoning_content":"hmm"}}]})),
            chunk(json!({"choices":[{"index":0,"delta":{"content":"Checking "}}]})),
            chunk(json!({"choices":[{"index":0,"delta":{"tool_calls":[
                {"index":0,"id":"call_a","type":"function","function":{"name":"weather","arguments":""}},
                {"index":1,"id":"call_b","type":"function","function":{"name":"time","arguments":"{}"}}
            ]}}]})),
            chunk(json!({"choices":[{"index":0,"delta":{"tool_calls":[
                {"index":0,"function":{"arguments":"{\"city\":\"Oslo\"}"}}
            ]}}]})),
            chunk(json!({"choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]})),
            chunk(json!({"choices":[],"usage":{"prompt_tokens":11,"completion_tokens":7,
                "prompt_tokens_details":{"cached_tokens":2}}})),
            "data: [DONE]\n\n".to_owned(),
        ]
        .concat();
        let server = serve(vec![Reply::sse(&body)]).await;
        let provider =
            connect(endpoint(ProviderKind::OpenAi, &server.base, Some("sk-o"))).expect("connects");
        let mut req = ChatRequest::new("gpt-x", vec![ChatMessage::user("hi")]);
        req.max_tokens = Some(100);
        req.system = Some("sys".into());
        req.tools = vec![ToolSpec {
            name: "weather".into(),
            description: "Weather".into(),
            input_schema: json!({"type":"object"}),
        }];
        let events: Vec<ChatEvent> = provider
            .stream(req)
            .await
            .expect("starts")
            .map(|e| e.expect("ok"))
            .collect()
            .await;
        assert_eq!(
            events,
            vec![
                ChatEvent::ThinkingDelta("hmm".into()),
                ChatEvent::TextDelta("Checking ".into()),
                ChatEvent::ToolUseStart {
                    id: "call_a".into(),
                    name: "weather".into()
                },
                ChatEvent::ToolUseStart {
                    id: "call_b".into(),
                    name: "time".into()
                },
                ChatEvent::ToolUseDelta {
                    id: "call_b".into(),
                    partial_json: "{}".into()
                },
                ChatEvent::ToolUseDelta {
                    id: "call_a".into(),
                    partial_json: "{\"city\":\"Oslo\"}".into()
                },
                ChatEvent::Usage(Usage {
                    input_tokens: 11,
                    output_tokens: 7,
                    cache_read_tokens: 2,
                    cache_write_tokens: 0
                }),
                ChatEvent::ToolUseEnd {
                    id: "call_a".into()
                },
                ChatEvent::ToolUseEnd {
                    id: "call_b".into()
                },
                ChatEvent::Stop(StopReason::ToolUse),
            ]
        );
        let req = server.requests().await.remove(0);
        assert!(
            req.head.starts_with("POST /v1/chat/completions "),
            "{}",
            req.head
        );
        assert!(
            req.head
                .to_lowercase()
                .contains("authorization: bearer sk-o")
        );
        let body: Value = serde_json::from_str(&req.body).expect("json");
        assert_eq!(body["max_completion_tokens"], 100);
        assert_eq!(body["stream_options"]["include_usage"], true);
        assert_eq!(
            body["messages"][0],
            json!({"role":"system","content":"sys"})
        );
        assert_eq!(body["tools"][0]["function"]["name"], "weather");
    }

    #[tokio::test]
    async fn local_servers_need_no_key_and_end_without_done() {
        let body = [
            chunk(json!({"choices":[{"index":0,"delta":{"content":"Hi"}}]})),
            chunk(json!({"choices":[{"index":0,"delta":{},"finish_reason":"length"}]})),
        ]
        .concat();
        let server = serve(vec![
            Reply::sse(&body),
            Reply::sse(&chunk(
                json!({"choices":[{"index":0,"delta":{"content":"x"}}]}),
            )),
        ])
        .await;
        let provider =
            connect(endpoint(ProviderKind::Strata, &server.base, None)).expect("connects");
        let mut req = ChatRequest::new("local", vec![ChatMessage::user("hi")]);
        req.max_tokens = Some(50);
        let events: Vec<_> = provider
            .stream(req.clone())
            .await
            .expect("starts")
            .collect()
            .await;
        assert!(matches!(
            events.last(),
            Some(Ok(ChatEvent::Stop(StopReason::MaxTokens)))
        ));
        let r = server.requests().await.remove(0);
        assert!(!r.head.to_lowercase().contains("authorization"));
        let body: Value = serde_json::from_str(&r.body).expect("json");
        assert_eq!(body["max_tokens"], 50);
        assert!(body.get("stream_options").is_none());

        // No finish reason and no [DONE]: the reply was cut off.
        let events: Vec<_> = provider.stream(req).await.expect("starts").collect().await;
        assert!(matches!(events.last(), Some(Err(LlmError::Stream(_)))));
    }

    #[tokio::test]
    async fn openai_without_a_key_fails_fast() {
        let provider = connect(endpoint(
            ProviderKind::OpenAi,
            "http://127.0.0.1:9/v1",
            None,
        ))
        .expect("connects");
        let err = provider
            .stream(ChatRequest::new("m", vec![]))
            .await
            .err()
            .expect("fails");
        assert!(matches!(err, LlmError::MissingKey));
    }

    #[tokio::test]
    async fn stream_errors_end_the_stream() {
        let body = chunk(json!({"error":{"message":"model crashed"}}));
        let server = serve(vec![Reply::sse(&body)]).await;
        let provider =
            connect(endpoint(ProviderKind::LmStudio, &server.base, None)).expect("connects");
        let events: Vec<_> = provider
            .stream(ChatRequest::new("m", vec![ChatMessage::user("x")]))
            .await
            .expect("starts")
            .collect()
            .await;
        assert_eq!(events.len(), 1);
        let err = events[0].as_ref().expect_err("fails").to_string();
        assert!(err.contains("model crashed"), "{err}");
    }

    #[tokio::test]
    async fn request_body_maps_images_documents_and_tool_turns() {
        let req = ChatRequest::new(
            "m",
            vec![
                ChatMessage {
                    role: Role::User,
                    content: vec![
                        ContentBlock::text("see"),
                        ContentBlock::Image {
                            media_type: "image/png".into(),
                            source: MediaSource::Base64 { data: "AAA".into() },
                        },
                    ],
                },
                ChatMessage {
                    role: Role::Assistant,
                    content: vec![
                        ContentBlock::Thinking {
                            text: "t".into(),
                            signature: None,
                            duration_ms: None,
                        },
                        ContentBlock::ToolUse {
                            id: "c1".into(),
                            name: "f".into(),
                            input_json: "{\"a\":1}".into(),
                        },
                    ],
                },
                ChatMessage {
                    role: Role::User,
                    content: vec![
                        ContentBlock::ToolResult {
                            tool_use_id: "c1".into(),
                            content: vec![ContentBlock::text("42")],
                            is_error: false,
                        },
                        ContentBlock::Document {
                            media_type: "text/plain".into(),
                            name: "n.txt".into(),
                            source: MediaSource::Base64 {
                                data: "aGk=".into(),
                            },
                        },
                    ],
                },
            ],
        );
        let body = request_body(ProviderKind::LmStudio, &req)
            .await
            .expect("builds");
        let m = body["messages"].as_array().expect("messages");
        assert_eq!(m.len(), 4);
        assert_eq!(
            m[0]["content"][1]["image_url"]["url"],
            "data:image/png;base64,AAA"
        );
        assert_eq!(m[1]["content"], Value::Null);
        assert_eq!(m[1]["tool_calls"][0]["function"]["arguments"], "{\"a\":1}");
        assert_eq!(
            m[2],
            json!({"role":"tool","tool_call_id":"c1","content":"42"})
        );
        assert_eq!(m[3]["content"], "<file name=\"n.txt\">\nhi\n</file>");
        assert!(body.get("max_tokens").is_none());
        assert!(body.get("tools").is_none());
        assert!(body.get("chat_template_kwargs").is_none());
        assert!(body.get("reasoning_effort").is_none());
    }

    #[tokio::test]
    async fn thinking_switch_effort_and_media_fallbacks() {
        let mut req = ChatRequest::new(
            "m",
            vec![ChatMessage {
                role: Role::User,
                content: vec![
                    ContentBlock::Image {
                        media_type: "image/png".into(),
                        source: MediaSource::Base64 { data: "AAA".into() },
                    },
                    ContentBlock::Document {
                        media_type: "application/pdf".into(),
                        name: "scan.pdf".into(),
                        source: MediaSource::Base64 {
                            data: "JVBERi0=".into(),
                        },
                    },
                    ContentBlock::text("what's this?"),
                ],
            }],
        );
        req.thinking = Thinking::Off;
        req.effort = Some(oxide_core::Effort::Xhigh);
        req.images = false;
        // Local servers: thinking off through the chat template; no reasoning_effort.
        let body = request_body(ProviderKind::Strata, &req)
            .await
            .expect("builds");
        assert_eq!(
            body["chat_template_kwargs"],
            json!({"enable_thinking": false})
        );
        assert!(body.get("reasoning_effort").is_none());
        let content = body["messages"][0]["content"].as_str().expect("text only");
        assert!(content.contains("can't see images"), "{content}");
        assert!(content.contains("scan.pdf"), "{content}");
        assert!(content.ends_with("what's this?"));
        // OpenAI: effort maps to the nearest reasoning_effort, no template switch.
        let body = request_body(ProviderKind::OpenAi, &req)
            .await
            .expect("builds");
        assert_eq!(body["reasoning_effort"], "high");
        assert!(body.get("chat_template_kwargs").is_none());
    }

    #[tokio::test]
    async fn lists_openai_models_through_the_curated_table() {
        let server = serve(vec![Reply::json(
            r#"{"object":"list","data":[{"id":"text-embedding-3-small"},{"id":"gpt-4.1"},{"id":"chatgpt-4o-latest"}]}"#,
        )])
        .await;
        let provider =
            connect(endpoint(ProviderKind::OpenAi, &server.base, Some("k"))).expect("connects");
        let listing = provider.list_models().await.expect("lists");
        let ids: Vec<_> = listing
            .models
            .iter()
            .map(|m| (m.model.as_str(), m.capabilities.tools))
            .collect();
        assert_eq!(ids, vec![("chatgpt-4o-latest", false), ("gpt-4.1", true)]);
        assert_eq!(listing.models[1].id, "openai/gpt-4.1");
    }

    #[tokio::test]
    async fn lmstudio_uses_its_native_model_list() {
        let server = serve(vec![Reply::json(
            r#"{"object":"list","data":[
                {"id":"qwen/qwen3-14b","type":"llm","capabilities":["tool_use"],"max_context_length":32768},
                {"id":"gemma-3","type":"vlm"},
                {"id":"nomic-embed","type":"embeddings"}
            ]}"#,
        )])
        .await;
        let provider =
            connect(endpoint(ProviderKind::LmStudio, &server.base, None)).expect("connects");
        let listing = provider.list_models().await.expect("lists");
        assert_eq!(listing.models.len(), 2);
        assert!(listing.models[0].capabilities.tools);
        assert_eq!(listing.models[0].context_window, Some(32768));
        assert!(!listing.models[1].capabilities.tools && listing.models[1].capabilities.images);
        let req = server.requests().await.remove(0);
        assert!(req.head.starts_with("GET /api/v0/models "), "{}", req.head);
    }

    #[tokio::test]
    async fn strata_models_are_unverified_until_probed() {
        let server = serve(vec![
            Reply::json(r#"{"data":[{"id":"strata-a"},{"id":"strata-embed"}]}"#),
            Reply::json(r#"{"choices":[{"message":{"role":"assistant","content":null,
                "tool_calls":[{"id":"c","type":"function","function":{"name":"ping","arguments":"{}"}}]}}]}"#),
            Reply::json(r#"{"choices":[{"message":{"role":"assistant","content":"pong"}}]}"#),
            Reply::status("400 Bad Request", r#"{"error":"tools not supported"}"#),
            Reply::status("500 Internal Server Error", r#"{"error":"boom"}"#),
        ])
        .await;
        let provider =
            connect(endpoint(ProviderKind::Strata, &server.base, None)).expect("connects");
        let listing = provider.list_models().await.expect("lists");
        assert_eq!(listing.unverified, vec!["strata-a"]);
        assert!(!listing.models[0].capabilities.tools);
        assert!(provider.probe_tools("strata-a").await.expect("probes"));
        assert!(!provider.probe_tools("strata-a").await.expect("probes"));
        assert!(!provider.probe_tools("strata-a").await.expect("probes"));
        assert!(provider.probe_tools("strata-a").await.is_err());
        let probe: Value = serde_json::from_str(&server.requests().await[1].body).expect("json");
        assert_eq!(probe["tools"][0]["function"]["name"], "ping");
        assert_eq!(probe["stream"], false);
    }
}
