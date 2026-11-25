//! Answers Utility Apps' SDK calls (`oxide.ai.text`, `oxide.web.fetch`, …).
//!
//! Every call names a method. Methods that reach models, the web, connectors or the
//! computer need a capability ([`method_capability`]) that the app's manifest declares and the
//! user allowed for the app; otherwise the call fails with
//! [`AppErrorCode::Undeclared`] or [`AppErrorCode::NeedsPermission`] (the frontend then asks
//! and calls again). Storage needs nothing.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::Engine as _;
use futures_util::StreamExt;
use oxide_core::model::split_model_id;
use oxide_core::{
    AppCallError, AppCapability, AppErrorCode, AppManifest, AppPrefs, ContentBlock, GenerationKind,
    MAX_APP_STORAGE_BYTES, McpContent, McpToolResult, MediaSource, ProviderKind, Role, StopReason,
    ThinkingStyle, ToolProgress, app_secret_name,
};
use oxide_llm::generation::InputImage;
use oxide_llm::{ChatEvent, ChatMessage, ChatRequest, Thinking};
use serde_json::{Value, json};
use tokio::sync::watch;

use super::UtilityApps;
use super::crawl::{CrawlRequest, crawl};
use crate::ModelRegistry;
use crate::tools::{Fetched, ToolOutput, Tools};

/// Hears a call's events (text deltas, progress) on their way to the app.
pub type CallEvents = Arc<dyn Fn(Value) + Send + Sync>;

/// Most bytes of base64 input one call may send (images, PDFs).
const MAX_INPUT_BYTES: usize = 40 * 1024 * 1024;
/// Most characters of a web page an app gets.
const MAX_PAGE_CHARS: usize = 400_000;

/// The capability an SDK method needs: `Some(None)` for none, `None` for no such method.
pub fn method_capability(method: &str) -> Option<Option<AppCapability>> {
    Some(Some(match method {
        "text.generate" | "text.edit" => AppCapability::Text,
        "media.image" => AppCapability::Image,
        "media.video" => AppCapability::Video,
        "media.music" => AppCapability::Music,
        "web.search" => AppCapability::WebSearch,
        "web.fetch" | "web.crawl" => AppCapability::WebFetch,
        "connectors.list" | "connectors.call" => AppCapability::Connectors,
        "python.run" => AppCapability::Python,
        "sandbox.exec" => AppCapability::Sandbox,
        "browser.tools" | "browser.call" => AppCapability::Browser,
        "chrome.tools" | "chrome.call" => AppCapability::Chrome,
        "folder.list" | "folder.read" | "folder.write" | "folder.mkdir" | "folder.delete"
        | "folder.rename" => AppCapability::Folder,
        "app.info" | "storage.get" | "storage.set" | "storage.delete" | "storage.keys"
        | "storage.clear" | "folder.get" => return Some(None),
        _ => return None,
    }))
}

fn failed(msg: impl Into<String>) -> AppCallError {
    AppCallError::failed(msg)
}

fn str_param<'a>(p: &'a Value, key: &str) -> Option<&'a str> {
    p.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Parses a model's JSON answer: plain, in a code fence, or the outermost object or array
/// in other text.
pub fn parse_json_answer(raw: &str) -> Result<Value, String> {
    let t = raw.trim();
    let t = t
        .strip_prefix("```json")
        .or_else(|| t.strip_prefix("```JSON"))
        .or_else(|| t.strip_prefix("```"))
        .map(|r| r.trim_end().trim_end_matches("```").trim())
        .unwrap_or(t);
    if let Ok(v) = serde_json::from_str::<Value>(t) {
        return Ok(v);
    }
    let start = t.find(['{', '[']);
    if let Some(s) = start {
        let close = if t[s..].starts_with('{') { '}' } else { ']' };
        if let Some(e) = t.rfind(close)
            && e > s
            && let Ok(v) = serde_json::from_str::<Value>(&t[s..=e])
        {
            return Ok(v);
        }
    }
    Err(if t.is_empty() {
        "the model's answer was empty".into()
    } else {
        "the model's answer wasn't valid JSON".into()
    })
}

/// Where one app request's time went, for `dev.log` (Phase 26 follow-up): how long the
/// server took before the first token (reading the prompt), how long the model thought
/// (thinking deltas before the answer) and how long it wrote, with the provider's token
/// counts. Output tokens the stream didn't show as text or thinking are reported as hidden
/// (a reasoning model whose thoughts the server doesn't stream).
pub(super) struct RequestStats {
    started: std::time::Instant,
    first_token: Option<std::time::Instant>,
    first_text: Option<std::time::Instant>,
    thinking_chars: usize,
    text_chars: usize,
    pub(super) usage: oxide_core::Usage,
}

impl RequestStats {
    pub(super) fn new() -> Self {
        Self {
            started: std::time::Instant::now(),
            first_token: None,
            first_text: None,
            thinking_chars: 0,
            text_chars: 0,
            usage: oxide_core::Usage::default(),
        }
    }

    pub(super) fn thinking(&mut self, t: &str) {
        self.first_token.get_or_insert_with(std::time::Instant::now);
        self.thinking_chars += t.chars().count();
    }

    pub(super) fn text(&mut self, t: &str) {
        let now = std::time::Instant::now();
        self.first_token.get_or_insert(now);
        self.first_text.get_or_insert(now);
        self.text_chars += t.chars().count();
    }

    /// The numbers, as `(field, value)` pairs in the order they are logged.
    pub(super) fn summary(&self, end: std::time::Instant) -> Vec<(&'static str, String)> {
        let secs = |a: std::time::Instant, b: std::time::Instant| {
            format!("{:.1}", b.saturating_duration_since(a).as_secs_f64())
        };
        let first = self.first_token.unwrap_or(end);
        let text_at = self.first_text.unwrap_or(end);
        // Roughly four characters a token: what the stream showed.
        let shown = (self.text_chars + self.thinking_chars).div_ceil(4);
        let hidden = (self.usage.output_tokens as usize).saturating_sub(shown);
        let writing = end.saturating_duration_since(first).as_secs_f64();
        let per_second = if writing > 0.0 && self.usage.output_tokens > 0 {
            format!("{:.1}", f64::from(self.usage.output_tokens) / writing)
        } else {
            "?".into()
        };
        vec![
            ("total_s", secs(self.started, end)),
            ("before_first_token_s", secs(self.started, first)),
            ("thinking_s", secs(first, text_at)),
            ("writing_s", secs(text_at, end)),
            ("input_tokens", self.usage.input_tokens.to_string()),
            (
                "cached_input_tokens",
                self.usage.cache_read_tokens.to_string(),
            ),
            ("output_tokens", self.usage.output_tokens.to_string()),
            ("thinking_chars", self.thinking_chars.to_string()),
            ("text_chars", self.text_chars.to_string()),
            ("hidden_output_tokens_est", hidden.to_string()),
            ("output_tokens_per_s", per_second),
        ]
    }

    fn log(&self, model: &str, structured: bool, thinking_off: bool, stop: Option<StopReason>) {
        let fields = self.summary(std::time::Instant::now());
        let line = fields
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(" ");
        tracing::info!(
            model = %model,
            structured,
            thinking_off,
            stop = ?stop,
            "app request: {line}"
        );
    }
}

/// Why a JSON answer that hit the output limit can't be used, and what helps.
fn cut_off(chars: usize) -> String {
    format!(
        "The model's answer was cut off before it was complete (it reached its output or \
         context limit after {chars} characters), so it isn't valid JSON. With LM Studio, load \
         the model with a larger context length (Context Length in its load settings); \
         otherwise ask for a smaller change or pick a model with a larger context window."
    )
}

/// Media in an input list: `[{ mediaType, data (base64), name? }]`.
fn media_inputs(p: &Value, key: &str) -> Result<Vec<(String, String, String)>, AppCallError> {
    let Some(list) = p.get(key) else {
        return Ok(Vec::new());
    };
    let Some(list) = list.as_array() else {
        return Err(failed(format!("“{key}” must be a list")));
    };
    let mut total = 0;
    let mut out = Vec::new();
    for (i, item) in list.iter().enumerate() {
        let media_type = str_param(item, "mediaType")
            .ok_or_else(|| failed(format!("{key}[{i}] has no mediaType")))?;
        let data =
            str_param(item, "data").ok_or_else(|| failed(format!("{key}[{i}] has no data")))?;
        total += data.len();
        if total > MAX_INPUT_BYTES {
            return Err(failed("The files sent are larger than 30 MB in all."));
        }
        let name = str_param(item, "name").unwrap_or("file").to_owned();
        out.push((media_type.to_owned(), data.to_owned(), name));
    }
    Ok(out)
}

/// A tool result as the SDK returns it.
fn tool_result_json(r: &McpToolResult) -> Value {
    let mut text = Vec::new();
    let mut images = Vec::new();
    for c in &r.content {
        match c {
            McpContent::Text { text: t } => text.push(t.clone()),
            McpContent::Image { data, mime_type } => {
                images.push(json!({ "mediaType": mime_type, "data": data }))
            }
            McpContent::Resource { text: Some(t), .. } => text.push(t.clone()),
            McpContent::ResourceLink { uri, .. } => text.push(uri.clone()),
            _ => {}
        }
    }
    json!({
        "text": text.join("\n"),
        "images": images,
        "structured": r.structured.as_deref().and_then(|s| serde_json::from_str::<Value>(s).ok()),
        "isError": r.is_error,
    })
}

/// Runs apps' SDK calls.
pub struct AppRuntime {
    apps: UtilityApps,
    registry: Arc<ModelRegistry>,
    tools: Tools,
    /// `<app data>/app-work`: a working folder per app for Python.
    work_dir: PathBuf,
    calls: Mutex<HashMap<String, watch::Sender<bool>>>,
}

impl AppRuntime {
    pub fn new(
        apps: UtilityApps,
        registry: Arc<ModelRegistry>,
        tools: Tools,
        work_dir: impl Into<PathBuf>,
    ) -> Self {
        Self {
            apps,
            registry,
            tools,
            work_dir: work_dir.into(),
            calls: Mutex::new(HashMap::new()),
        }
    }

    pub fn apps(&self) -> &UtilityApps {
        &self.apps
    }

    /// Stops a running call.
    pub fn cancel(&self, call_id: &str) {
        if let Some(tx) = lock(&self.calls).get(call_id) {
            let _ = tx.send(true);
        }
    }

    /// The chat model an app's text calls use: its setting, else the manifest's (when that
    /// model is available), else the first available model.
    pub async fn text_model(&self, manifest: &AppManifest, prefs: &AppPrefs) -> Option<String> {
        if let Some(m) = prefs.text_model.as_deref().filter(|m| !m.trim().is_empty()) {
            return Some(m.to_owned());
        }
        if let Some(m) = manifest.models.text.as_deref()
            && self.registry.find(m).await.is_some()
        {
            return Some(m.to_owned());
        }
        let listings = self.registry.list(false).await.ok()?;
        listings
            .into_iter()
            .flat_map(|l| l.models)
            .find(|m| m.capabilities.tools)
            .map(|m| m.id)
    }

    /// Runs one call. `events` hears its streamed text and progress.
    pub async fn call(
        &self,
        app_id: &str,
        call_id: &str,
        method: &str,
        params: Value,
        events: CallEvents,
    ) -> Result<Value, AppCallError> {
        let need = method_capability(method).ok_or_else(|| {
            AppCallError::new(
                AppErrorCode::Unknown,
                format!("oxide has no method “{method}”"),
            )
        })?;
        // The builder's Preview runs the draft (`draft:<id>`) with the app's settings.
        let (app_id, draft) = super::builder::split_ref(app_id);
        let manifest = self
            .apps
            .manifest_at(app_id, draft)
            .await
            .map_err(|e| failed(e.to_string()))?;
        let prefs = self
            .apps
            .prefs(app_id)
            .await
            .map_err(|e| failed(e.to_string()))?;
        if let Some(cap) = need {
            if !manifest.declares(cap) {
                return Err(AppCallError {
                    code: AppErrorCode::Undeclared,
                    message: format!(
                        "The app's manifest doesn't declare the “{}” capability.",
                        cap.as_str()
                    ),
                    capability: Some(cap),
                });
            }
            if !prefs.allows(cap) {
                return Err(AppCallError {
                    code: AppErrorCode::NeedsPermission,
                    message: format!("{} wants to {}.", manifest.name, cap.describe()),
                    capability: Some(cap),
                });
            }
        }
        let (tx, mut rx) = watch::channel(false);
        lock(&self.calls).insert(call_id.to_owned(), tx);
        let work = self.dispatch(
            app_id,
            &manifest,
            &prefs,
            method,
            &params,
            events,
            rx.clone(),
        );
        let out = tokio::select! {
            r = work => r,
            _ = rx.wait_for(|c| *c) => Err(AppCallError::new(AppErrorCode::Cancelled, "Stopped.")),
        };
        lock(&self.calls).remove(call_id);
        out
    }

    #[allow(clippy::too_many_arguments)]
    async fn dispatch(
        &self,
        app_id: &str,
        manifest: &AppManifest,
        prefs: &AppPrefs,
        method: &str,
        p: &Value,
        events: CallEvents,
        cancel: watch::Receiver<bool>,
    ) -> Result<Value, AppCallError> {
        let settings = self
            .apps
            .store()
            .app_settings()
            .await
            .map_err(|e| failed(e.to_string()))?;
        match method {
            "app.info" => Ok(json!({
                "textModel": self.text_model(manifest, prefs).await,
                "granted": prefs.granted,
            })),
            "text.generate" => self.text(manifest, prefs, p, events).await,
            "text.edit" => self.text_edit(manifest, prefs, p, events).await,
            "media.image" => self.media(app_id, manifest, prefs, GenerationKind::Image, p, &settings, events).await,
            "media.video" => self.media(app_id, manifest, prefs, GenerationKind::Video, p, &settings, events).await,
            "media.music" => self.media(app_id, manifest, prefs, GenerationKind::Music, p, &settings, events).await,
            "web.search" => {
                let query = str_param(p, "query").ok_or_else(|| failed("web.search needs a query"))?;
                let count = p
                    .get("count")
                    .and_then(Value::as_u64)
                    .map(|n| n.clamp(1, 20) as u32)
                    .unwrap_or(settings.web_search.max_results);
                let hits = self
                    .tools
                    .web
                    .search(&settings.web_search, query, count, None)
                    .await
                    .map_err(|e| failed(format!("The search failed: {e}")))?;
                Ok(json!(hits.into_iter().map(|h| json!({
                    "title": h.title, "url": h.url, "snippet": h.snippet, "age": h.age,
                })).collect::<Vec<_>>()))
            }
            "web.fetch" => {
                let url = str_param(p, "url").ok_or_else(|| failed("web.fetch needs a url"))?;
                if str_param(p, "format") == Some("html") {
                    let page = self.tools.web.fetch_raw(url).await.map_err(|e| failed(format!("The page couldn't be read: {e}")))?;
                    let content: String = page.text.chars().take(MAX_PAGE_CHARS).collect();
                    return Ok(json!({
                        "url": page.url, "title": Value::Null, "content": content,
                        "contentType": page.content_type, "truncated": page.truncated,
                    }));
                }
                match self.tools.web.fetch(url).await.map_err(|e| failed(format!("The page couldn't be read: {e}")))? {
                    Fetched::Text { url, title, text, content_type } => {
                        let truncated = text.chars().count() > MAX_PAGE_CHARS;
                        let content: String = text.chars().take(MAX_PAGE_CHARS).collect();
                        Ok(json!({ "url": url, "title": title, "content": content, "contentType": content_type, "truncated": truncated }))
                    }
                    Fetched::Image { url, media_type, data_base64 } => Ok(json!({
                        "url": url, "title": Value::Null, "content": "", "contentType": media_type, "image": data_base64,
                    })),
                }
            }
            "web.crawl" => {
                let url = str_param(p, "url").ok_or_else(|| failed("web.crawl needs a url"))?;
                let req = CrawlRequest {
                    start: url.to_owned(),
                    max_pages: p.get("maxPages").and_then(Value::as_u64).unwrap_or(10) as usize,
                    max_depth: p.get("maxDepth").and_then(Value::as_u64).unwrap_or(2).min(10) as u32,
                    html: str_param(p, "format") == Some("html"),
                };
                let stop = cancel.clone();
                let r = crawl(&self.tools.web, &req, &move || *stop.borrow())
                    .await
                    .map_err(|e| failed(format!("The site couldn't be crawled: {e}")))?;
                serde_json::to_value(r).map_err(|e| failed(e.to_string()))
            }
            "connectors.list" => {
                let mcp = self.tools.mcp().ok_or_else(|| failed("Connectors aren't available."))?;
                Ok(json!(mcp.tools().into_iter().map(|t| json!({
                    "server": t.server, "name": t.name, "title": t.title, "description": t.description,
                    "inputSchema": serde_json::from_str::<Value>(&t.input_schema).unwrap_or(Value::Null),
                    "readOnly": t.annotations.read_only_hint == Some(true),
                })).collect::<Vec<_>>()))
            }
            "connectors.call" => {
                let mcp = self.tools.mcp().ok_or_else(|| failed("Connectors aren't available."))?;
                let server = str_param(p, "server").ok_or_else(|| failed("connectors.call needs a server"))?;
                let tool = str_param(p, "tool").ok_or_else(|| failed("connectors.call needs a tool"))?;
                let args = p.get("arguments").cloned().unwrap_or_else(|| json!({}));
                let r = mcp
                    .call(server, tool, args, None)
                    .await
                    .map_err(|e| failed(format!("The tool couldn't run: {e}")))?;
                Ok(tool_result_json(&r))
            }
            "python.run" => {
                let env = self.tools.python().ok_or_else(|| failed("Python isn't available in this build."))?;
                if !settings.python.enabled {
                    return Err(failed("Python is switched off in Settings → Python."));
                }
                let code = str_param(p, "code").ok_or_else(|| failed("python.run needs code"))?;
                let secs = p
                    .get("timeoutSeconds")
                    .and_then(Value::as_u64)
                    .unwrap_or(u64::from(settings.python.timeout_secs))
                    .clamp(1, 3_600);
                let out = env
                    .run(oxide_pyenv::RunRequest {
                        code: code.to_owned(),
                        cwd: self.work_dir.join(app_id),
                        timeout: Duration::from_secs(secs),
                    })
                    .await
                    .map_err(|e| failed(format!("Python couldn't run: {e}")))?;
                Ok(json!({
                    "exitCode": out.exit_code,
                    "stdout": String::from_utf8_lossy(&out.stdout),
                    "stderr": String::from_utf8_lossy(&out.stderr),
                    "timedOut": out.timed_out,
                }))
            }
            "sandbox.exec" => {
                let sb = self.tools.sandboxes().ok_or_else(|| failed("The sandbox isn't available in this build."))?;
                if !settings.sandbox.enabled {
                    return Err(failed("The sandbox is switched off in Settings → Sandbox."));
                }
                let command = str_param(p, "command").ok_or_else(|| failed("sandbox.exec needs a command"))?;
                let cwd = str_param(p, "cwd").unwrap_or("");
                let secs = p
                    .get("timeoutSeconds")
                    .and_then(Value::as_u64)
                    .unwrap_or(u64::from(settings.sandbox.timeout_secs))
                    .clamp(1, u64::from(settings.sandbox.timeout_secs.max(1)));
                let r = sb
                    .exec(&format!("app-{app_id}"), command, cwd, Duration::from_secs(secs))
                    .await
                    .map_err(|e| failed(format!("The command couldn't run: {e}")))?;
                Ok(json!({
                    "exitCode": r.code, "stdout": r.stdout, "stderr": r.stderr,
                    "timedOut": r.timed_out, "cwd": r.cwd,
                }))
            }
            "browser.tools" => Ok(json!(crate::tools::browser::browser_entries()
                .iter()
                .map(|e| json!({ "name": e.tool(), "description": e.description, "inputSchema": e.schema }))
                .collect::<Vec<_>>())),
            "browser.call" => {
                let b = self.tools.browser().ok_or_else(|| failed("The browser isn't available in this build."))?;
                if !settings.browser.enabled {
                    return Err(failed("The browser agent is switched off in Settings → Browser."));
                }
                let tool = str_param(p, "tool").ok_or_else(|| failed("browser.call needs a tool"))?;
                let args = p.get("arguments").cloned().unwrap_or_else(|| json!({}));
                let llm = match self.text_model(manifest, prefs).await {
                    Some(m) => self.browser_llm(&m).await,
                    None => None,
                };
                let id = uuid::Uuid::now_v7().to_string();
                let r = b
                    .call(&format!("app-{app_id}"), tool, args, &id, llm)
                    .await
                    .map_err(|e| failed(e.to_string()))?;
                Ok(tool_result_json(&r))
            }
            "chrome.tools" => Ok(json!(crate::tools::chrome::chrome_entries()
                .iter()
                .map(|e| json!({ "name": e.tool(), "description": e.description, "inputSchema": e.schema }))
                .collect::<Vec<_>>())),
            "chrome.call" => {
                let c = self.tools.chrome().ok_or_else(|| failed("Chrome isn't available in this build."))?;
                if !settings.chrome.enabled {
                    return Err(failed("Chrome is switched off in Settings → Chrome."));
                }
                let tool = str_param(p, "tool").ok_or_else(|| failed("chrome.call needs a tool"))?;
                let args = p.get("arguments").cloned().unwrap_or_else(|| json!({}));
                let r = c
                    .call(&format!("app-{app_id}"), tool, args)
                    .await
                    .map_err(|e| failed(e.to_string()))?;
                Ok(tool_result_json(&r))
            }
            m if m.starts_with("folder.") => {
                super::folder::call(prefs, self.apps.protected(), m, p).await
            }
            "storage.get" => {
                let key = storage_key(p)?;
                let v = self.apps.store().app_storage_get(app_id, key).await.map_err(|e| failed(e.to_string()))?;
                Ok(v.and_then(|s| serde_json::from_str::<Value>(&s).ok()).unwrap_or(Value::Null))
            }
            "storage.set" => {
                let key = storage_key(p)?;
                let value = p.get("value").cloned().unwrap_or(Value::Null);
                let text = serde_json::to_string(&value).map_err(|e| failed(e.to_string()))?;
                self.apps
                    .store()
                    .app_storage_set(app_id, key, &text, MAX_APP_STORAGE_BYTES)
                    .await
                    .map_err(|e| failed(format!("The value couldn't be stored: {e}")))?;
                Ok(Value::Null)
            }
            "storage.delete" => {
                let key = storage_key(p)?;
                self.apps.store().app_storage_delete(app_id, key).await.map_err(|e| failed(e.to_string()))?;
                Ok(Value::Null)
            }
            "storage.keys" => Ok(json!(self.apps.store().app_storage_keys(app_id).await.map_err(|e| failed(e.to_string()))?)),
            "storage.clear" => {
                self.apps.store().app_storage_clear(app_id).await.map_err(|e| failed(e.to_string()))?;
                Ok(Value::Null)
            }
            _ => Err(AppCallError::new(AppErrorCode::Unknown, format!("oxide has no method “{method}”"))),
        }
    }

    async fn browser_llm(&self, model_id: &str) -> Option<oxide_pyenv::BrowserLlm> {
        let (provider_id, model) = split_model_id(model_id)?;
        let (config, api_key) = self.registry.endpoint(provider_id).await.ok()?;
        let vision = match (self.registry.find(model_id).await, config.kind) {
            (Some(i), _) => i.capabilities.images,
            (None, ProviderKind::Anthropic) => {
                oxide_llm::capabilities::anthropic_curated(model).images
            }
            (None, _) => false,
        };
        Some(oxide_pyenv::BrowserLlm {
            kind: config.kind,
            base_url: config.base_url,
            model: model.to_owned(),
            api_key,
            vision,
        })
    }

    pub(super) async fn text(
        &self,
        manifest: &AppManifest,
        prefs: &AppPrefs,
        p: &Value,
        events: CallEvents,
    ) -> Result<Value, AppCallError> {
        let model_id = self.text_model(manifest, prefs).await.ok_or_else(|| {
            failed("No chat model is available. Add an endpoint in Settings → Providers.")
        })?;
        let (provider_id, model) = split_model_id(&model_id)
            .ok_or_else(|| failed(format!("“{model_id}” isn't a model id")))?;
        let (config, provider) = self
            .registry
            .provider(provider_id)
            .await
            .map_err(|e| failed(format!("The model {model_id} isn't available: {e}")))?;
        let caps = self
            .registry
            .find(&model_id)
            .await
            .map(|m| m.capabilities)
            .unwrap_or_default();

        let mut messages: Vec<ChatMessage> = Vec::new();
        if let Some(list) = p.get("messages").and_then(Value::as_array) {
            for m in list {
                let role = match m.get("role").and_then(Value::as_str) {
                    Some("assistant") => Role::Assistant,
                    _ => Role::User,
                };
                let text = m
                    .get("content")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned();
                messages.push(ChatMessage {
                    role,
                    content: vec![ContentBlock::text(text)],
                });
            }
        }
        if let Some(prompt) = p.get("prompt").and_then(Value::as_str) {
            messages.push(ChatMessage::user(prompt));
        }
        if messages.last().is_none_or(|m| m.role != Role::User) {
            return Err(failed(
                "ai.text needs a prompt or messages ending with the user's",
            ));
        }
        let mut extra = Vec::new();
        for (media_type, data, _) in media_inputs(p, "images")? {
            extra.push(ContentBlock::Image {
                media_type,
                source: MediaSource::Base64 { data },
            });
        }
        for (media_type, data, name) in media_inputs(p, "documents")? {
            extra.push(ContentBlock::Document {
                media_type,
                name,
                source: MediaSource::Base64 { data },
            });
        }
        if let Some(last) = messages.last_mut() {
            // Files first, like a chat message with attachments.
            extra.append(&mut last.content);
            last.content = extra;
        }

        let json_mode = match p.get("json") {
            None | Some(Value::Null) | Some(Value::Bool(false)) => None,
            Some(Value::Bool(true)) => Some(None),
            Some(schema) => Some(Some(schema.clone())),
        };
        let mut system = p
            .get("system")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        if let Some(schema) = &json_mode {
            if !system.is_empty() {
                system.push_str("\n\n");
            }
            system.push_str(
                "Reply with exactly one JSON value and nothing else: no Markdown code fences, \
                 no explanations.",
            );
            if let Some(schema) = schema {
                system.push_str(&format!(" It must match this JSON Schema:\n{schema}"));
            }
        }
        let stream = p.get("stream").and_then(Value::as_bool).unwrap_or(false);
        let want_thinking = p.get("thinking").and_then(Value::as_bool).unwrap_or(false);
        // The schema also goes to the provider as structured output (OpenAI-compatible
        // `response_format`, Anthropic `output_config.format`), so servers that constrain
        // sampling with it (LM Studio, llama.cpp) can only answer with matching JSON.
        let schema = json_mode.clone().flatten();
        let base = |messages: Vec<ChatMessage>, structured: bool| {
            let mut r = ChatRequest::new(model, messages);
            r.system = (!system.is_empty()).then(|| system.clone());
            if structured {
                r.json_schema = schema.clone();
            }
            r.images = caps.images || config.kind == ProviderKind::Anthropic;
            r.native_pdfs = config.kind == ProviderKind::Anthropic;
            r.max_tokens = p
                .get("maxTokens")
                .and_then(Value::as_u64)
                .map(|n| n.clamp(16, 200_000) as u32);
            if !want_thinking
                && ((config.kind == ProviderKind::Anthropic
                    && caps.thinking == ThinkingStyle::Adaptive
                    && caps.thinking_off)
                    || caps.thinking == ThinkingStyle::Switch)
            {
                r.thinking = Thinking::Off;
            }
            r
        };
        let run = |request: ChatRequest, live: bool| {
            let provider = provider.clone();
            let events = events.clone();
            let model_id = model_id.clone();
            async move {
                let structured = request.json_schema.is_some();
                let thinking_off = request.thinking == Thinking::Off;
                let mut stats = RequestStats::new();
                let mut s = provider
                    .stream(request)
                    .await
                    .map_err(|e| failed(format!("The model couldn't answer: {e}")))?;
                let mut text = String::new();
                let mut stop = None;
                while let Some(ev) = s.next().await {
                    match ev.map_err(|e| failed(format!("The model couldn't answer: {e}")))? {
                        ChatEvent::TextDelta(t) => {
                            stats.text(&t);
                            if live {
                                events(json!({ "type": "delta", "text": t }));
                            }
                            text.push_str(&t);
                        }
                        ChatEvent::ThinkingDelta(t) => stats.thinking(&t),
                        ChatEvent::Usage(u) => stats.usage.merge(u),
                        ChatEvent::Stop(reason) => {
                            stop = Some(reason);
                            break;
                        }
                        _ => {}
                    }
                }
                stats.log(&model_id, structured, thinking_off, stop);
                Ok::<(String, Option<StopReason>), AppCallError>((text, stop))
            }
        };
        let live = stream && json_mode.is_none();
        let (text, stop) = match run(base(messages.clone(), true), live).await {
            Err(e) if schema.is_some() => {
                // A server that refuses structured output: ask once more without it (the
                // system prompt still asks for the JSON).
                tracing::info!(model = %model_id, error = %e.message, "structured output refused");
                run(base(messages.clone(), false), live).await?
            }
            r => r?,
        };
        let Some(_) = json_mode else {
            // `truncated`: the answer stopped at the output limit (it may be incomplete).
            return Ok(json!({
                "text": text,
                "model": model_id,
                "truncated": stop == Some(StopReason::MaxTokens),
            }));
        };
        if stop == Some(StopReason::MaxTokens) && parse_json_answer(&text).is_err() {
            // Asking again gets the same cut-off answer.
            return Err(failed(cut_off(text.chars().count())));
        }
        match parse_json_answer(&text) {
            Ok(v) => Ok(json!({ "text": text, "json": v, "model": model_id })),
            Err(first) => {
                // One more try, telling the model what went wrong.
                let mut retry = messages;
                retry.push(ChatMessage {
                    role: Role::Assistant,
                    content: vec![ContentBlock::text(if text.trim().is_empty() {
                        "(empty)".to_owned()
                    } else {
                        text.clone()
                    })],
                });
                retry.push(ChatMessage::user(format!(
                    "That wasn't usable: {first}. Reply again with only the JSON value."
                )));
                let (again, stop) = run(base(retry, true), false).await?;
                if stop == Some(StopReason::MaxTokens) && parse_json_answer(&again).is_err() {
                    return Err(failed(cut_off(again.chars().count())));
                }
                let v = parse_json_answer(&again).map_err(|e| {
                    failed(format!("The model's answer couldn't be read as JSON ({e}). Try again or pick another model."))
                })?;
                Ok(json!({ "text": again, "json": v, "model": model_id }))
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn media(
        &self,
        app_id: &str,
        manifest: &AppManifest,
        prefs: &AppPrefs,
        kind: GenerationKind,
        p: &Value,
        settings: &oxide_core::AppSettings,
        events: CallEvents,
    ) -> Result<Value, AppCallError> {
        let generation = self
            .tools
            .generation()
            .ok_or_else(|| failed("Media generation isn't available in this build."))?;
        let over = prefs.generation(kind);
        let ep = over.apply(
            settings.generation.endpoint(kind),
            manifest.models.generation(kind),
        );
        if ep.model.trim().is_empty() {
            return Err(failed(format!(
                "No {} model is set. Choose one in Settings → Media generation or in the app's settings.",
                kind.as_str()
            )));
        }
        // The app's own key first; the global key only when the app keeps the global URL.
        let mut keys = vec![app_secret_name(app_id, kind)];
        if over.base_url.trim().is_empty() && over.provider_id.is_none() {
            keys.push(kind.secret_name());
        }
        let mut references = Vec::new();
        let key = if kind == GenerationKind::Video {
            "reference"
        } else {
            "references"
        };
        let list = match p.get(key) {
            Some(Value::Object(_)) => json!({ "x": [p[key].clone()] }),
            Some(v) => json!({ "x": v }),
            None => json!({ "x": [] }),
        };
        for (i, (media_type, data, name)) in media_inputs(&list, "x")?.into_iter().enumerate() {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(data.as_bytes())
                .map_err(|_| failed(format!("reference image {} isn't base64", i + 1)))?;
            references.push(InputImage {
                name,
                media_type,
                bytes,
            });
        }
        let input = match kind {
            GenerationKind::Image => json!({
                "prompt": p.get("prompt"), "n": p.get("n"), "size": p.get("size"),
                "quality": p.get("quality"), "transparent_background": p.get("transparent"),
            }),
            GenerationKind::Video => json!({
                "prompt": p.get("prompt"), "seconds": p.get("seconds"), "size": p.get("size"),
            }),
            GenerationKind::Music => json!({
                "style": p.get("style").or_else(|| p.get("prompt")), "lyrics": p.get("lyrics"),
                "duration_seconds": p.get("durationSeconds"), "seed": p.get("seed"),
            }),
        };
        let progress = move |pr: ToolProgress| {
            events(json!({ "type": "progress", "percent": pr.percent, "note": pr.note }));
        };
        let out: ToolOutput = generation
            .run_on(kind, &input, &ep, keys, references, &progress)
            .await;
        let mut text = Vec::new();
        let mut files = Vec::new();
        for b in &out.content {
            match b {
                ContentBlock::Text { text: t } => text.push(t.clone()),
                ContentBlock::Image {
                    media_type,
                    source: MediaSource::Path { path },
                } => files.push(
                    json!({ "name": file_name(path), "mediaType": media_type, "path": path }),
                ),
                ContentBlock::Document {
                    media_type,
                    name,
                    source: MediaSource::Path { path },
                } => files.push(json!({ "name": name, "mediaType": media_type, "path": path })),
                _ => {}
            }
        }
        if out.is_error || files.is_empty() {
            return Err(failed(text.join(" ")));
        }
        Ok(json!({ "files": files, "model": ep.model, "text": text.join("\n") }))
    }
}

fn file_name(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or("file").to_owned()
}

fn storage_key(p: &Value) -> Result<&str, AppCallError> {
    let key = p
        .get("key")
        .and_then(Value::as_str)
        .ok_or_else(|| failed("storage needs a key"))?;
    if key.is_empty() || key.len() > 200 {
        return Err(failed("storage keys must be 1–200 characters"));
    }
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_answers_are_found_in_fences_and_prose() {
        assert_eq!(
            parse_json_answer("{\"a\":1}").expect("plain"),
            json!({"a":1})
        );
        assert_eq!(
            parse_json_answer("```json\n{\"a\":2}\n```").expect("fenced"),
            json!({"a":2})
        );
        assert_eq!(
            parse_json_answer("Here you go:\n[1, 2]\nEnjoy").expect("in prose"),
            json!([1, 2])
        );
        assert!(parse_json_answer("no json here").is_err());
        assert!(parse_json_answer("  ").unwrap_err().contains("empty"));
    }

    #[test]
    fn request_stats_split_the_time_and_estimate_hidden_thinking() {
        use std::time::Duration;
        let mut st = RequestStats::new();
        let t0 = st.started;
        st.first_token = Some(t0 + Duration::from_secs(40));
        st.first_text = Some(t0 + Duration::from_secs(100));
        st.thinking_chars = 400;
        st.text_chars = 1600;
        st.usage = oxide_core::Usage {
            input_tokens: 12_000,
            output_tokens: 2_500,
            cache_read_tokens: 8_000,
            cache_write_tokens: 0,
        };
        let got: std::collections::HashMap<_, _> = st
            .summary(t0 + Duration::from_secs(150))
            .into_iter()
            .collect();
        assert_eq!(got["total_s"], "150.0");
        assert_eq!(got["before_first_token_s"], "40.0");
        assert_eq!(got["thinking_s"], "60.0");
        assert_eq!(got["writing_s"], "50.0");
        assert_eq!(got["input_tokens"], "12000");
        assert_eq!(got["cached_input_tokens"], "8000");
        // 2,000 characters shown ≈ 500 tokens; the other 2,000 weren't streamed.
        assert_eq!(got["hidden_output_tokens_est"], "2000");
        // 2,500 tokens in the 110 s after the first one.
        assert_eq!(got["output_tokens_per_s"], "22.7");
    }

    #[test]
    fn methods_map_to_capabilities() {
        assert_eq!(
            method_capability("text.generate"),
            Some(Some(AppCapability::Text))
        );
        assert_eq!(
            method_capability("text.edit"),
            Some(Some(AppCapability::Text))
        );
        assert_eq!(
            method_capability("web.crawl"),
            Some(Some(AppCapability::WebFetch))
        );
        assert_eq!(method_capability("storage.set"), Some(None));
        assert_eq!(method_capability("folder.get"), Some(None));
        assert_eq!(
            method_capability("folder.write"),
            Some(Some(AppCapability::Folder))
        );
        assert_eq!(method_capability("fs.readHostFile"), None);
    }
}
