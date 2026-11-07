//! Anthropic Messages API: streaming over SSE, extended thinking, tool use, images, PDFs and
//! prompt-cache breakpoints. Requests that hit 429/529 are retried with backoff.

use std::collections::HashMap;

use futures_util::FutureExt;
use futures_util::future::BoxFuture;
use oxide_core::{ContentBlock, ModelInfo, StopReason, Usage, model::model_id};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::capabilities::anthropic_from_api;
use crate::http::{describe, send_with_retry, streaming_client};
use crate::media::{IMAGE_OMITTED, base64_of, is_text, pdf_as_text, text_of};
use crate::sse::SseEvent;
use crate::stream::{Decoder, Out, sse_stream};
use crate::{
    ChatEvent, ChatRequest, ChatStream, Endpoint, LlmError, ModelListing, Provider, Thinking,
};

/// `anthropic-version` header value.
pub const ANTHROPIC_VERSION: &str = "2026-10-21";

/// Output limit used when the request doesn't set one.
pub const DEFAULT_MAX_TOKENS: u32 = 8_192;

/// Adds the auth and version headers. The API takes the key as a bearer token (the older
/// `x-api-key` header still works, but the current API version documents bearer auth).
pub(crate) fn with_auth(
    req: reqwest::RequestBuilder,
    key: Option<&str>,
) -> reqwest::RequestBuilder {
    let req = req.header("anthropic-version", ANTHROPIC_VERSION);
    match key {
        Some(k) => req.bearer_auth(k),
        None => req,
    }
}

pub struct AnthropicProvider {
    endpoint: Endpoint,
    client: reqwest::Client,
}

impl AnthropicProvider {
    pub fn new(endpoint: Endpoint) -> Result<Self, LlmError> {
        Ok(Self {
            endpoint,
            client: streaming_client()?,
        })
    }

    fn key(&self) -> Result<&str, LlmError> {
        self.endpoint.api_key().ok_or(LlmError::MissingKey)
    }

    async fn models(&self) -> Result<ModelListing, LlmError> {
        let key = self.key()?;
        let base = &self.endpoint.base_url;
        let mut models = Vec::new();
        let mut after: Option<String> = None;
        // The API pages at up to 1000 models; a few pages at most in practice.
        for _ in 0..10 {
            let mut url = format!("{base}/models?limit=1000");
            if let Some(a) = &after {
                url.push_str("&after_id=");
                url.push_str(a);
            }
            let req = with_auth(self.client.get(&url), Some(key))
                .timeout(std::time::Duration::from_secs(20));
            let res = send_with_retry(req, base).await?;
            let page: ModelPage = res
                .json()
                .await
                .map_err(|e| LlmError::Decode(format!("model list: {}", describe(&e))))?;
            for m in page.data {
                if m.lifecycle.as_deref() == Some("retired") {
                    continue;
                }
                models.push(ModelInfo {
                    id: model_id(&self.endpoint.id, &m.id),
                    provider_id: self.endpoint.id.clone(),
                    capabilities: anthropic_from_api(&m.id, m.capabilities.as_ref()),
                    label: m.display_name.unwrap_or_else(|| m.id.clone()),
                    model: m.id,
                    context_window: m.max_input_tokens,
                    max_output_tokens: m.max_tokens,
                });
            }
            match (page.has_more, page.last_id) {
                (true, Some(last)) => after = Some(last),
                _ => break,
            }
        }
        Ok(ModelListing {
            models,
            unverified: Vec::new(),
        })
    }

    async fn start(&self, request: ChatRequest) -> Result<ChatStream, LlmError> {
        let key = self.key()?;
        let body = request_body(&request).await?;
        let url = format!("{}/messages", self.endpoint.base_url);
        let req = with_auth(self.client.post(&url), Some(key))
            .header("accept", "text/event-stream")
            .json(&body);
        let res = send_with_retry(req, &self.endpoint.base_url).await?;
        Ok(sse_stream(res, AnthropicDecoder::default()))
    }
}

impl Provider for AnthropicProvider {
    fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    fn list_models(&self) -> BoxFuture<'_, Result<ModelListing, LlmError>> {
        self.models().boxed()
    }

    fn stream(&self, request: ChatRequest) -> BoxFuture<'_, Result<ChatStream, LlmError>> {
        self.start(request).boxed()
    }

    fn probe_tools<'a>(&'a self, _model: &'a str) -> BoxFuture<'a, Result<bool, LlmError>> {
        // Every Claude model the API lists supports tools.
        async { Ok(true) }.boxed()
    }
}

#[derive(Deserialize)]
struct ModelPage {
    data: Vec<ModelEntry>,
    #[serde(default)]
    has_more: bool,
    last_id: Option<String>,
}

#[derive(Deserialize)]
struct ModelEntry {
    id: String,
    display_name: Option<String>,
    lifecycle: Option<String>,
    max_input_tokens: Option<u32>,
    max_tokens: Option<u32>,
    capabilities: Option<Value>,
}

/// Builds the Messages API request body.
pub(crate) async fn request_body(req: &ChatRequest) -> Result<Value, LlmError> {
    let mut messages: Vec<Value> = Vec::new();
    for m in &req.messages {
        let blocks = convert_blocks(&m.content, req).await?;
        if !blocks.is_empty() {
            messages.push(json!({ "role": m.role.as_str(), "content": blocks }));
        }
    }
    if req.cache {
        add_message_breakpoints(&mut messages);
    }

    let mut body = Map::new();
    body.insert("model".into(), json!(req.model));
    body.insert(
        "max_tokens".into(),
        json!(req.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS)),
    );
    body.insert("stream".into(), json!(true));
    // One breakpoint for the system prompt (Anthropic allows four per request): on the
    // knowledge block when there is one, since the prompt after it changes every minute.
    let mut system: Vec<Value> = Vec::new();
    if let Some(context) = req
        .system_context
        .as_deref()
        .filter(|s| !s.trim().is_empty())
    {
        let mut block = json!({ "type": "text", "text": context });
        if req.cache {
            block["cache_control"] = ephemeral();
        }
        system.push(block);
    }
    if let Some(text) = req.system.as_deref().filter(|s| !s.trim().is_empty()) {
        let mut block = json!({ "type": "text", "text": text });
        if req.cache && system.is_empty() {
            block["cache_control"] = ephemeral();
        }
        system.push(block);
    }
    if !system.is_empty() {
        body.insert("system".into(), json!(system));
    }
    if !req.tools.is_empty() || !req.server_tools.is_empty() {
        let mut tools: Vec<Value> = req
            .tools
            .iter()
            .map(|t| {
                json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.input_schema,
                })
            })
            .chain(req.server_tools.iter().cloned())
            .collect();
        if req.cache
            && let Some(last) = tools.last_mut()
        {
            last["cache_control"] = ephemeral();
        }
        body.insert("tools".into(), json!(tools));
    }
    match req.thinking {
        Thinking::Default => {}
        Thinking::Off => {
            body.insert("thinking".into(), json!({ "type": "disabled" }));
        }
        Thinking::Budget(budget) => {
            body.insert(
                "thinking".into(),
                json!({ "type": "enabled", "budget_tokens": budget, "display": "summarized" }),
            );
        }
        Thinking::Adaptive => {
            body.insert(
                "thinking".into(),
                json!({ "type": "adaptive", "display": "summarized" }),
            );
        }
    }
    if let Some(effort) = req.effort {
        body.insert("output_config".into(), json!({ "effort": effort.as_str() }));
    }
    body.insert("messages".into(), json!(messages));
    Ok(Value::Object(body))
}

fn ephemeral() -> Value {
    json!({ "type": "ephemeral" })
}

/// Cache breakpoints on the last block of the final turn and of the user turn before it, so
/// each new turn reads the previous prefix from cache. (System and tools get their own; the
/// API allows four.)
fn add_message_breakpoints(messages: &mut [Value]) {
    let user_turns: Vec<usize> = messages
        .iter()
        .enumerate()
        .filter(|(_, m)| m["role"] == "user")
        .map(|(i, _)| i)
        .collect();
    let mut marks: Vec<usize> = user_turns.iter().rev().take(2).copied().collect();
    if let Some(last) = messages.len().checked_sub(1)
        && !marks.contains(&last)
    {
        marks.truncate(1);
        marks.push(last);
    }
    for i in marks {
        if let Some(block) = messages[i]["content"]
            .as_array_mut()
            .and_then(|blocks| blocks.last_mut())
        {
            block["cache_control"] = ephemeral();
        }
    }
}

async fn convert_blocks(
    content: &[ContentBlock],
    req: &ChatRequest,
) -> Result<Vec<Value>, LlmError> {
    let mut out = Vec::new();
    for block in content {
        match block {
            ContentBlock::Text { text } => {
                if !text.trim().is_empty() {
                    out.push(json!({ "type": "text", "text": text }));
                }
            }
            ContentBlock::Image { media_type, source } => {
                if req.images {
                    out.push(json!({
                        "type": "image",
                        "source": { "type": "base64", "media_type": media_type, "data": base64_of(source).await? },
                    }));
                } else {
                    out.push(json!({ "type": "text", "text": IMAGE_OMITTED }));
                }
            }
            ContentBlock::Document {
                media_type,
                name,
                source,
            } => {
                if media_type == "application/pdf" && !req.native_pdfs {
                    out.push(json!({ "type": "text", "text": pdf_as_text(name, source).await }));
                } else if media_type == "application/pdf" {
                    out.push(json!({
                        "type": "document",
                        "title": name,
                        "source": { "type": "base64", "media_type": media_type, "data": base64_of(source).await? },
                    }));
                } else if is_text(media_type) {
                    out.push(json!({
                        "type": "document",
                        "title": name,
                        "source": { "type": "text", "media_type": "text/plain", "data": text_of(source).await? },
                    }));
                } else {
                    out.push(json!({
                        "type": "text",
                        "text": format!("[Attached file {name} ({media_type}) can't be read by this model.]"),
                    }));
                }
            }
            ContentBlock::Thinking {
                text, signature, ..
            } => {
                // Only signed thinking can be sent back.
                if let Some(sig) = signature {
                    out.push(json!({ "type": "thinking", "thinking": text, "signature": sig }));
                }
            }
            ContentBlock::ToolUse {
                id,
                name,
                input_json,
                ..
            } => {
                let input: Value = serde_json::from_str(input_json).unwrap_or_else(|_| json!({}));
                out.push(json!({ "type": "tool_use", "id": id, "name": name, "input": input }));
            }
            ContentBlock::ServerToolUse {
                id,
                name,
                input_json,
            } => {
                let input: Value = serde_json::from_str(input_json).unwrap_or_else(|_| json!({}));
                out.push(
                    json!({ "type": "server_tool_use", "id": id, "name": name, "input": input }),
                );
            }
            ContentBlock::ServerToolResult { raw_json, .. } => {
                if let Ok(mut v) = serde_json::from_str::<Value>(raw_json) {
                    if let Some(o) = v.as_object_mut() {
                        o.remove("cache_control");
                    }
                    out.push(v);
                }
            }
            ContentBlock::ToolResult {
                tool_use_id,
                content,
                is_error,
            } => {
                let inner = Box::pin(convert_blocks(content, req)).await?;
                out.push(json!({
                    "type": "tool_result",
                    "tool_use_id": tool_use_id,
                    "content": inner,
                    "is_error": is_error,
                }));
            }
            ContentBlock::ArtifactRef { title, version, .. } => out.push(json!({
                "type": "text",
                "text": format!("[Artifact: {title}, version {version}]"),
            })),
        }
    }
    Ok(out)
}

fn stop_reason(s: &str) -> StopReason {
    match s {
        "end_turn" => StopReason::EndTurn,
        "max_tokens" => StopReason::MaxTokens,
        "stop_sequence" => StopReason::StopSequence,
        "tool_use" => StopReason::ToolUse,
        "pause_turn" => StopReason::PauseTurn,
        "refusal" => StopReason::Refusal,
        "model_context_window_exceeded" => StopReason::ContextFull,
        _ => StopReason::Other,
    }
}

fn usage_of(v: &Value) -> Usage {
    let n = |k: &str| {
        v.get(k)
            .and_then(Value::as_u64)
            .map(|x| u32::try_from(x).unwrap_or(u32::MAX))
            .unwrap_or(0)
    };
    Usage {
        input_tokens: n("input_tokens"),
        output_tokens: n("output_tokens"),
        cache_read_tokens: n("cache_read_input_tokens"),
        cache_write_tokens: n("cache_creation_input_tokens"),
    }
}

/// Decodes Messages API stream events.
#[derive(Default)]
pub(crate) struct AnthropicDecoder {
    /// Content block index → tool call id.
    tools: HashMap<u64, String>,
    usage: Usage,
    stop: Option<StopReason>,
    finished: bool,
}

impl Decoder for AnthropicDecoder {
    fn event(&mut self, event: SseEvent, out: &mut Out) -> bool {
        let Ok(v) = serde_json::from_str::<Value>(&event.data) else {
            // Unparseable data: tolerate unless it's an error event.
            if event.event.as_deref() == Some("error") {
                out.push_back(Err(LlmError::Stream(format!(
                    "Anthropic reported an error: {}",
                    event.data.chars().take(200).collect::<String>()
                ))));
                return true;
            }
            return false;
        };
        let ty = v
            .get("type")
            .and_then(Value::as_str)
            .or(event.event.as_deref())
            .unwrap_or("");
        let index = v.get("index").and_then(Value::as_u64).unwrap_or(0);
        match ty {
            "message_start" => {
                if let Some(u) = v.pointer("/message/usage") {
                    self.usage.merge(usage_of(u));
                    out.push_back(Ok(ChatEvent::Usage(self.usage)));
                }
            }
            "content_block_start" => {
                let block = &v["content_block"];
                match block["type"].as_str().unwrap_or("") {
                    "tool_use" => {
                        let id = block["id"].as_str().unwrap_or_default().to_owned();
                        let name = block["name"].as_str().unwrap_or_default().to_owned();
                        self.tools.insert(index, id.clone());
                        out.push_back(Ok(ChatEvent::ToolUseStart { id, name }));
                    }
                    "text" => {
                        if let Some(t) = block["text"].as_str().filter(|t| !t.is_empty()) {
                            out.push_back(Ok(ChatEvent::TextDelta(t.to_owned())));
                        }
                    }
                    "thinking" => {
                        if let Some(t) = block["thinking"].as_str().filter(|t| !t.is_empty()) {
                            out.push_back(Ok(ChatEvent::ThinkingDelta(t.to_owned())));
                        }
                    }
                    "server_tool_use" => {
                        let id = block["id"].as_str().unwrap_or_default().to_owned();
                        let name = block["name"].as_str().unwrap_or_default().to_owned();
                        self.tools.insert(index, id.clone());
                        out.push_back(Ok(ChatEvent::ServerToolStart { id, name }));
                        // Some proxies send the whole input in the start block.
                        if let Some(input) = block
                            .get("input")
                            .filter(|i| i.as_object().is_some_and(|o| !o.is_empty()))
                            && let Some(id) = self.tools.get(&index)
                        {
                            out.push_back(Ok(ChatEvent::ToolUseDelta {
                                id: id.clone(),
                                partial_json: input.to_string(),
                            }));
                        }
                    }
                    t if t.ends_with("_tool_result") => {
                        let tool_use_id =
                            block["tool_use_id"].as_str().unwrap_or_default().to_owned();
                        out.push_back(Ok(ChatEvent::ServerToolResult {
                            tool_use_id,
                            raw_json: block.to_string(),
                        }));
                    }
                    _ => {} // redacted thinking: not shown
                }
            }
            "content_block_delta" => {
                let delta = &v["delta"];
                match delta["type"].as_str().unwrap_or("") {
                    "text_delta" => {
                        if let Some(t) = delta["text"].as_str() {
                            out.push_back(Ok(ChatEvent::TextDelta(t.to_owned())));
                        }
                    }
                    "thinking_delta" => {
                        if let Some(t) = delta["thinking"].as_str() {
                            out.push_back(Ok(ChatEvent::ThinkingDelta(t.to_owned())));
                        }
                    }
                    "signature_delta" => {
                        if let Some(s) = delta["signature"].as_str() {
                            out.push_back(Ok(ChatEvent::ThinkingSignature(s.to_owned())));
                        }
                    }
                    "input_json_delta" => {
                        if let (Some(id), Some(p)) =
                            (self.tools.get(&index), delta["partial_json"].as_str())
                        {
                            out.push_back(Ok(ChatEvent::ToolUseDelta {
                                id: id.clone(),
                                partial_json: p.to_owned(),
                            }));
                        }
                    }
                    _ => {}
                }
            }
            "content_block_stop" => {
                if let Some(id) = self.tools.remove(&index) {
                    out.push_back(Ok(ChatEvent::ToolUseEnd { id }));
                }
            }
            "message_delta" => {
                if let Some(r) = v.pointer("/delta/stop_reason").and_then(Value::as_str) {
                    self.stop = Some(stop_reason(r));
                }
                if let Some(u) = v.get("usage") {
                    self.usage.merge(usage_of(u));
                    out.push_back(Ok(ChatEvent::Usage(self.usage)));
                }
            }
            "message_stop" => {
                self.finished = true;
                out.push_back(Ok(ChatEvent::Stop(
                    self.stop.unwrap_or(StopReason::EndTurn),
                )));
                return true;
            }
            "error" => {
                self.finished = true;
                let kind = v.pointer("/error/type").and_then(Value::as_str);
                let message = v
                    .pointer("/error/message")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown error");
                let err = match kind {
                    Some("overloaded_error") => LlmError::Overloaded,
                    _ => LlmError::Stream(format!("Anthropic reported an error: {message}")),
                };
                out.push_back(Err(err));
                return true;
            }
            _ => {} // ping and future event types
        }
        false
    }

    fn eof(&mut self, out: &mut Out) {
        if !self.finished {
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
    use oxide_core::{MediaSource, ProviderKind, Role};

    fn endpoint(base: &str) -> Endpoint {
        Endpoint::new(
            "anthropic",
            ProviderKind::Anthropic,
            base,
            Some("sk-ant-test".into()),
        )
    }

    const STREAM: &str = concat!(
        "event: message_start\n",
        "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"usage\":{\"input_tokens\":25,\"cache_read_input_tokens\":3,\"output_tokens\":1}}}\n\n",
        "event: content_block_start\n",
        "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"thinking\",\"thinking\":\"\"}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"Let me think\"}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"signature_delta\",\"signature\":\"sig==\"}}\n\n",
        "event: content_block_stop\n",
        "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
        "event: ping\n",
        "data: {\"type\":\"ping\"}\n\n",
        "event: content_block_start\n",
        "data: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"text_delta\",\"text\":\" world\"}}\n\n",
        "event: content_block_stop\n",
        "data: {\"type\":\"content_block_stop\",\"index\":1}\n\n",
        "event: content_block_start\n",
        "data: {\"type\":\"content_block_start\",\"index\":2,\"content_block\":{\"type\":\"tool_use\",\"id\":\"toolu_1\",\"name\":\"search\",\"input\":{}}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":2,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"q\\\":\"}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":2,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"\\\"rust\\\"}\"}}\n\n",
        "event: content_block_stop\n",
        "data: {\"type\":\"content_block_stop\",\"index\":2}\n\n",
        "event: message_delta\n",
        "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"},\"usage\":{\"output_tokens\":42}}\n\n",
        "event: message_stop\n",
        "data: {\"type\":\"message_stop\"}\n\n",
    );

    async fn collect(stream: ChatStream) -> Vec<Result<ChatEvent, LlmError>> {
        stream.collect().await
    }

    #[tokio::test]
    async fn streams_thinking_text_tools_usage_and_stop() {
        let server = serve(vec![Reply::sse(STREAM)]).await;
        let provider = connect(endpoint(&server.base)).expect("connects");
        let mut req = ChatRequest::new("claude-x", vec![ChatMessage::user("hi")]);
        req.thinking = Thinking::Budget(2048);
        req.max_tokens = Some(4096);
        let events = collect(provider.stream(req).await.expect("starts")).await;
        let events: Vec<ChatEvent> = events.into_iter().map(|e| e.expect("ok")).collect();
        assert_eq!(
            events,
            vec![
                ChatEvent::Usage(Usage {
                    input_tokens: 25,
                    output_tokens: 1,
                    cache_read_tokens: 3,
                    cache_write_tokens: 0
                }),
                ChatEvent::ThinkingDelta("Let me think".into()),
                ChatEvent::ThinkingSignature("sig==".into()),
                ChatEvent::TextDelta("Hello".into()),
                ChatEvent::TextDelta(" world".into()),
                ChatEvent::ToolUseStart {
                    id: "toolu_1".into(),
                    name: "search".into()
                },
                ChatEvent::ToolUseDelta {
                    id: "toolu_1".into(),
                    partial_json: "{\"q\":".into()
                },
                ChatEvent::ToolUseDelta {
                    id: "toolu_1".into(),
                    partial_json: "\"rust\"}".into()
                },
                ChatEvent::ToolUseEnd {
                    id: "toolu_1".into()
                },
                ChatEvent::Usage(Usage {
                    input_tokens: 25,
                    output_tokens: 42,
                    cache_read_tokens: 3,
                    cache_write_tokens: 0
                }),
                ChatEvent::Stop(StopReason::ToolUse),
            ]
        );

        let req = server.requests().await.remove(0);
        assert!(req.head.starts_with("POST /v1/messages "), "{}", req.head);
        let head = req.head.to_lowercase();
        assert!(head.contains("authorization: bearer sk-ant-test"));
        assert!(head.contains(&format!("anthropic-version: {ANTHROPIC_VERSION}")));
        let body: Value = serde_json::from_str(&req.body).expect("json body");
        assert_eq!(body["model"], "claude-x");
        assert_eq!(body["stream"], true);
        assert_eq!(body["max_tokens"], 4096);
        assert_eq!(
            body["thinking"],
            json!({"type": "enabled", "budget_tokens": 2048, "display": "summarized"})
        );
        assert!(body.get("temperature").is_none());
    }

    #[tokio::test]
    async fn server_tools_stream_and_go_back_unchanged() {
        let stream = concat!(
            "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":5}}}\n\n",
            "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"server_tool_use\",\"id\":\"srvtoolu_1\",\"name\":\"web_search\"}}\n\n",
            "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"query\\\":\\\"oxide\\\"}\"}}\n\n",
            "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
            "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"web_search_tool_result\",\"tool_use_id\":\"srvtoolu_1\",\"content\":[{\"type\":\"web_search_result\",\"title\":\"Oxide\",\"url\":\"https://example.com\",\"encrypted_content\":\"abc\"}]}}\n\n",
            "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":1}\n\n",
            "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"pause_turn\"},\"usage\":{\"output_tokens\":9}}\n\n",
            "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
        );
        let server = serve(vec![Reply::sse(stream)]).await;
        let provider = connect(endpoint(&server.base)).expect("connects");
        let mut req = ChatRequest::new("claude-x", vec![ChatMessage::user("hi")]);
        req.server_tools = vec![json!({"type": "web_search_20250305", "name": "web_search"})];
        let events: Vec<ChatEvent> = collect(provider.stream(req).await.expect("starts"))
            .await
            .into_iter()
            .map(|e| e.expect("ok"))
            .collect();
        assert!(events.contains(&ChatEvent::ServerToolStart {
            id: "srvtoolu_1".into(),
            name: "web_search".into()
        }));
        assert!(events.contains(&ChatEvent::ToolUseDelta {
            id: "srvtoolu_1".into(),
            partial_json: "{\"query\":\"oxide\"}".into()
        }));
        let raw = events
            .iter()
            .find_map(|e| match e {
                ChatEvent::ServerToolResult {
                    tool_use_id,
                    raw_json,
                } => {
                    assert_eq!(tool_use_id, "srvtoolu_1");
                    Some(raw_json.clone())
                }
                _ => None,
            })
            .expect("a result");
        assert_eq!(events.last(), Some(&ChatEvent::Stop(StopReason::PauseTurn)));
        let body: Value =
            serde_json::from_str(&server.requests().await.remove(0).body).expect("json");
        assert_eq!(body["tools"][0]["type"], "web_search_20250305");

        // The paused turn goes back with the server blocks as they came.
        let mut req = ChatRequest::new("claude-x", vec![ChatMessage::user("hi")]);
        req.messages.push(ChatMessage {
            role: Role::Assistant,
            content: vec![
                ContentBlock::ServerToolUse {
                    id: "srvtoolu_1".into(),
                    name: "web_search".into(),
                    input_json: "{\"query\":\"oxide\"}".into(),
                },
                ContentBlock::ServerToolResult {
                    tool_use_id: "srvtoolu_1".into(),
                    raw_json: raw,
                },
            ],
        });
        let body = request_body(&req).await.expect("body");
        let blocks = &body["messages"][1]["content"];
        assert_eq!(blocks[0]["type"], "server_tool_use");
        assert_eq!(blocks[0]["input"]["query"], "oxide");
        assert_eq!(blocks[1]["type"], "web_search_tool_result");
        assert_eq!(blocks[1]["content"][0]["encrypted_content"], "abc");
    }

    #[tokio::test]
    async fn retries_overloaded_then_succeeds() {
        let server = serve(vec![
            Reply::status(
                "529 Overloaded",
                r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#,
            )
            .header("retry-after", "0"),
            Reply::status(
                "429 Too Many Requests",
                r#"{"type":"error","error":{"type":"rate_limit_error","message":"slow down"}}"#,
            )
            .header("retry-after", "0"),
            Reply::sse(STREAM),
        ])
        .await;
        let provider = connect(endpoint(&server.base)).expect("connects");
        let events = collect(
            provider
                .stream(ChatRequest::new("claude-x", vec![ChatMessage::user("hi")]))
                .await
                .expect("starts after retries"),
        )
        .await;
        assert!(matches!(events.last(), Some(Ok(ChatEvent::Stop(_)))));
        assert_eq!(server.requests().await.len(), 3);
    }

    #[tokio::test]
    async fn errors_before_the_stream_are_readable_and_keyless() {
        let server = serve(vec![Reply::status(
            "404 Not Found",
            r#"{"type":"error","error":{"type":"not_found_error","message":"model: nope"}}"#,
        )])
        .await;
        let provider = connect(endpoint(&server.base)).expect("connects");
        let err = match provider
            .stream(ChatRequest::new("nope", vec![ChatMessage::user("hi")]))
            .await
        {
            Err(e) => e,
            Ok(_) => panic!("should fail"),
        };
        assert_eq!(err.to_string(), "the server answered 404: model: nope");
        assert!(!format!("{err:?}").contains("sk-ant-test"));
    }

    #[tokio::test]
    async fn stream_error_events_and_cut_connections_fail_the_stream() {
        let overloaded = concat!(
            "event: content_block_delta\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hi\"}}\n\n",
            "event: error\n",
            "data: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"Overloaded\"}}\n\n",
        );
        let cut = "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hi\"}}\n\n";
        let server = serve(vec![Reply::sse(overloaded), Reply::sse(cut)]).await;
        let provider = connect(endpoint(&server.base)).expect("connects");
        for expected in ["overloaded", "closed before"] {
            let events = collect(
                provider
                    .stream(ChatRequest::new("m", vec![ChatMessage::user("hi")]))
                    .await
                    .expect("starts"),
            )
            .await;
            assert_eq!(events.len(), 2, "{events:?}");
            assert!(matches!(&events[0], Ok(ChatEvent::TextDelta(t)) if t == "Hi"));
            let err = events[1].as_ref().expect_err("fails").to_string();
            assert!(err.contains(expected), "{err}");
        }
    }

    #[tokio::test]
    async fn missing_key_is_reported_without_a_request() {
        let e = Endpoint::new("a", ProviderKind::Anthropic, "http://127.0.0.1:9/v1", None);
        let provider = connect(e).expect("connects");
        let err = provider
            .stream(ChatRequest::new("m", vec![]))
            .await
            .err()
            .expect("fails");
        assert!(matches!(err, LlmError::MissingKey));
    }

    #[tokio::test]
    async fn request_body_carries_media_tools_cache_and_thinking() {
        let mut req = ChatRequest::new(
            "claude-x",
            vec![
                ChatMessage {
                    role: Role::User,
                    content: vec![
                        ContentBlock::text("look"),
                        ContentBlock::Image {
                            media_type: "image/png".into(),
                            source: MediaSource::Base64 {
                                data: "iVBO".into(),
                            },
                        },
                        ContentBlock::Document {
                            media_type: "application/pdf".into(),
                            name: "a.pdf".into(),
                            source: MediaSource::Base64 {
                                data: "JVBE".into(),
                            },
                        },
                    ],
                },
                ChatMessage {
                    role: Role::Assistant,
                    content: vec![
                        ContentBlock::Thinking {
                            text: "unsigned".into(),
                            signature: None,
                            duration_ms: None,
                        },
                        ContentBlock::text("ok"),
                    ],
                },
                ChatMessage::user("next"),
            ],
        );
        req.system = Some("Be brief.".into());
        req.cache = true;
        req.thinking = Thinking::Adaptive;
        req.tools = vec![ToolSpec {
            name: "search".into(),
            description: "Search".into(),
            input_schema: json!({"type": "object"}),
        }];
        let body = request_body(&req).await.expect("builds");
        assert_eq!(body["system"][0]["cache_control"]["type"], "ephemeral");
        assert_eq!(body["tools"][0]["cache_control"]["type"], "ephemeral");
        assert_eq!(body["tools"][0]["input_schema"]["type"], "object");
        assert_eq!(
            body["thinking"],
            json!({"type": "adaptive", "display": "summarized"})
        );
        let msgs = body["messages"].as_array().expect("messages");
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0]["content"][1]["source"]["data"], "iVBO");
        assert_eq!(msgs[0]["content"][2]["type"], "document");
        assert_eq!(msgs[0]["content"][2]["title"], "a.pdf");
        // Unsigned thinking is dropped; the text remains.
        assert_eq!(msgs[1]["content"].as_array().map(Vec::len), Some(1));
        // Breakpoints on the last two user turns, nowhere else.
        assert_eq!(msgs[0]["content"][2]["cache_control"]["type"], "ephemeral");
        assert_eq!(msgs[2]["content"][0]["cache_control"]["type"], "ephemeral");
        assert!(msgs[1]["content"][0].get("cache_control").is_none());

        req.thinking = Thinking::Off;
        req.cache = false;
        let body = request_body(&req).await.expect("builds");
        assert_eq!(body["thinking"], json!({"type": "disabled"}));
        assert!(body["system"][0].get("cache_control").is_none());
        assert!(body.get("output_config").is_none());

        // Project knowledge: its own first system block, carrying the breakpoint.
        req.cache = true;
        req.system_context = Some("<project_knowledge>…</project_knowledge>".into());
        let body = request_body(&req).await.expect("builds");
        let system = body["system"].as_array().expect("blocks");
        assert_eq!(system.len(), 2);
        assert_eq!(
            system[0]["text"],
            "<project_knowledge>…</project_knowledge>"
        );
        assert_eq!(system[0]["cache_control"]["type"], "ephemeral");
        assert!(system[1].get("cache_control").is_none());
        assert_eq!(system[1]["text"], "Be brief.");
        req.system_context = None;
        req.cache = false;

        // Effort, and media for a model that can't see images or read PDFs natively.
        req.effort = Some(oxide_core::Effort::Low);
        req.images = false;
        req.native_pdfs = false;
        let body = request_body(&req).await.expect("builds");
        assert_eq!(body["output_config"], json!({"effort": "low"}));
        let first = &body["messages"][0]["content"];
        assert_eq!(first[1]["type"], "text");
        assert!(
            first[1]["text"]
                .as_str()
                .is_some_and(|t| t.contains("can't see images"))
        );
        assert_eq!(first[2]["type"], "text");
        assert!(
            first[2]["text"]
                .as_str()
                .is_some_and(|t| t.contains("a.pdf"))
        );
    }

    #[tokio::test]
    async fn lists_models_with_capabilities_and_skips_retired() {
        let page = r#"{"data":[
            {"type":"model","id":"claude-opus-5-5","display_name":"Claude Opus 5.5","lifecycle":"active",
             "max_input_tokens":1000000,"max_tokens":128000,
             "capabilities":{"image_input":{"supported":true},"pdf_input":{"supported":true},
               "thinking":{"supported":true,"types":{"adaptive":{"supported":true},"enabled":{"supported":true}}}}},
            {"type":"model","id":"claude-3-haiku-20240307","display_name":"Claude Haiku 3","capabilities":null},
            {"type":"model","id":"claude-2.1","display_name":"Old","lifecycle":"retired"}
        ],"has_more":false,"first_id":null,"last_id":null}"#;
        let server = serve(vec![Reply::json(page)]).await;
        let provider = connect(endpoint(&server.base)).expect("connects");
        let listing = provider.list_models().await.expect("lists");
        assert_eq!(listing.models.len(), 2);
        let opus = &listing.models[0];
        assert_eq!(opus.id, "anthropic/claude-opus-5-5");
        assert_eq!(opus.label, "Claude Opus 5.5");
        assert_eq!(opus.max_output_tokens, Some(128_000));
        assert_eq!(
            opus.capabilities.thinking,
            oxide_core::ThinkingStyle::Adaptive
        );
        assert!(listing.models[1].capabilities.tools);
        let req = server.requests().await.remove(0);
        assert!(
            req.head.starts_with("GET /v1/models?limit=1000 "),
            "{}",
            req.head
        );
    }
}
