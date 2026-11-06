//! Streamable HTTP: every message is its own POST; the answer is JSON or an SSE stream that
//! carries related notifications before the response.
//!
//! - Modern servers (2026-07-28): `MCP-Protocol-Version`, `Mcp-Method` and `Mcp-Name` on
//!   every request, no sessions, no GET stream.
//! - Legacy servers (2025-03-26 … 2025-11-25): `Mcp-Session-Id` from `initialize`, a
//!   standalone GET stream for server-initiated messages, DELETE on close.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::future::BoxFuture;
use oxide_core::{McpEra, McpLogSource};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::Value;
use tokio::sync::watch;
use tokio::task::JoinHandle;

use super::{InboundTx, ServerMessage};
use crate::McpError;
use crate::jsonrpc::{self, Incoming, RequestId, RpcError};
use crate::log::ServerLog;
use crate::protocol::{self, encode_header_value};
use crate::sse::SseParser;

/// Longest error body kept for messages.
const MAX_ERROR_BODY: usize = 600;

/// Bearer tokens for servers that need a sign-in (see `oauth`).
pub trait AuthProvider: Send + Sync {
    /// The current access token, refreshed first if it has expired. `None`: not signed in.
    fn bearer(&self) -> BoxFuture<'_, Result<Option<String>, McpError>>;
    /// The server rejected the token. `Ok(true)` if a new one is ready to try.
    fn refresh(&self) -> BoxFuture<'_, Result<bool, McpError>>;
}

#[derive(Debug, Clone, Default)]
pub struct HttpConfig {
    pub url: String,
    /// Extra headers from the config (e.g. a static API key).
    pub headers: Vec<(String, String)>,
}

#[derive(Default)]
struct State {
    era: Option<McpEra>,
    version: Option<String>,
    session: Option<String>,
}

struct Inner {
    client: reqwest::Client,
    url: String,
    headers: HeaderMap,
    auth: Option<Arc<dyn AuthProvider>>,
    state: Mutex<State>,
    inbound: InboundTx,
    log: Arc<ServerLog>,
    closed_tx: watch::Sender<Option<String>>,
}

pub struct HttpTransport {
    inner: Arc<Inner>,
    closed: watch::Receiver<Option<String>>,
    get_task: Mutex<Option<JoinHandle<()>>>,
}

impl HttpTransport {
    pub fn new(
        config: &HttpConfig,
        auth: Option<Arc<dyn AuthProvider>>,
        inbound: InboundTx,
        log: Arc<ServerLog>,
    ) -> Result<Self, McpError> {
        let parsed = url::Url::parse(config.url.trim())
            .map_err(|_| McpError::Config(format!("\"{}\" isn't a valid URL", config.url)))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(McpError::Config(
                "the URL must start with http:// or https://".into(),
            ));
        }
        let mut headers = HeaderMap::new();
        for (k, v) in &config.headers {
            let name = HeaderName::from_bytes(k.trim().as_bytes())
                .map_err(|_| McpError::Config(format!("\"{k}\" isn't a valid header name")))?;
            let value = HeaderValue::from_str(v.trim())
                .map_err(|_| McpError::Config(format!("the value of header \"{k}\" is invalid")))?;
            headers.insert(name, value);
        }
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .user_agent(format!("Oxide/{}", oxide_core::version()))
            .build()
            .map_err(|e| McpError::Network(e.to_string()))?;
        let (closed_tx, closed) = watch::channel(None);
        Ok(Self {
            inner: Arc::new(Inner {
                client,
                url: parsed.to_string(),
                headers,
                auth,
                state: Mutex::new(State::default()),
                inbound,
                log,
                closed_tx,
            }),
            closed,
            get_task: Mutex::new(None),
        })
    }

    pub fn url(&self) -> &str {
        &self.inner.url
    }

    pub fn set_protocol(&self, era: McpEra, version: &str) {
        let mut s = self.inner.state();
        s.era = Some(era);
        s.version = (!version.is_empty()).then(|| version.to_owned());
        if era == McpEra::Modern {
            s.session = None;
        }
    }

    pub fn closed(&self) -> watch::Receiver<Option<String>> {
        self.closed.clone()
    }

    pub async fn request(
        &self,
        id: &RequestId,
        method: &str,
        params: Value,
        extra: &[(String, String)],
    ) -> Result<Value, McpError> {
        let body = jsonrpc::request(id, method, &params);
        let name = protocol::name_param(method, &params);
        self.inner
            .post_request(id, method, name.as_deref(), &body, extra)
            .await
    }

    pub async fn notify(&self, method: &str, params: Value) -> Result<(), McpError> {
        let body = jsonrpc::notification(method, &params);
        self.inner.post_one_way(method, &body).await
    }

    pub async fn respond(
        &self,
        id: &RequestId,
        outcome: Result<Value, RpcError>,
    ) -> Result<(), McpError> {
        let body = jsonrpc::response(id, &outcome);
        self.inner.post_one_way("", &body).await
    }

    /// Legacy servers may push requests and notifications on a GET stream.
    pub fn start_get_stream(&self) {
        let inner = self.inner.clone();
        let task = tokio::spawn(async move { inner.get_stream_loop().await });
        if let Some(old) = self
            .get_task
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .replace(task)
        {
            old.abort();
        }
    }

    pub async fn close(&self) {
        if let Some(t) = self
            .get_task
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            t.abort();
        }
        let session = self.inner.state().session.take();
        if let Some(session) = session {
            let mut req = self
                .inner
                .client
                .delete(&self.inner.url)
                .headers(self.inner.headers.clone())
                .header("Mcp-Session-Id", session);
            let version = self.inner.state().version.clone();
            if let Some(v) = version {
                req = req.header("MCP-Protocol-Version", v);
            }
            let _ = tokio::time::timeout(Duration::from_secs(3), req.send()).await;
        }
        self.inner
            .closed_tx
            .send_replace(Some(" (disconnected)".into()));
    }
}

impl Drop for HttpTransport {
    fn drop(&mut self) {
        if let Some(t) = self
            .get_task
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            t.abort();
        }
    }
}

impl Inner {
    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Headers every message carries: config headers, protocol version, session, auth.
    async fn base_request(
        &self,
        method: &str,
        http_method: reqwest::Method,
        accept: &str,
    ) -> Result<reqwest::RequestBuilder, McpError> {
        let (era, version, session) = {
            let s = self.state();
            (s.era, s.version.clone(), s.session.clone())
        };
        let mut req = self
            .client
            .request(http_method, &self.url)
            .headers(self.headers.clone())
            .header(reqwest::header::ACCEPT, accept);
        if let Some(v) = version.filter(|_| method != "initialize") {
            req = req.header("MCP-Protocol-Version", v);
        }
        if era == Some(McpEra::Legacy)
            && let Some(s) = session
        {
            req = req.header("Mcp-Session-Id", s);
        }
        if era == Some(McpEra::Modern) && !method.is_empty() {
            req = req.header("Mcp-Method", method);
        }
        if let Some(auth) = &self.auth
            && !self.headers.contains_key(reqwest::header::AUTHORIZATION)
            && let Some(token) = auth.bearer().await?
        {
            req = req.bearer_auth(token);
        }
        Ok(req)
    }

    async fn post_request(
        &self,
        id: &RequestId,
        method: &str,
        name: Option<&str>,
        body: &Value,
        extra: &[(String, String)],
    ) -> Result<Value, McpError> {
        let mut refreshed = false;
        loop {
            let mut req = self
                .base_request(
                    method,
                    reqwest::Method::POST,
                    "application/json, text/event-stream",
                )
                .await?
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(body.to_string());
            let modern = self.state().era == Some(McpEra::Modern);
            if modern {
                if let Some(n) = name {
                    req = req.header("Mcp-Name", encode_header_value(n));
                }
                for (k, v) in extra {
                    req = req.header(k.as_str(), v.as_str());
                }
            }
            let resp = req.send().await?;
            let status = resp.status();
            if status == reqwest::StatusCode::UNAUTHORIZED || is_scope_challenge(&resp) {
                let challenge = www_authenticate(&resp);
                if !refreshed && let Some(auth) = &self.auth {
                    refreshed = true;
                    if auth.refresh().await.unwrap_or(false) {
                        continue;
                    }
                }
                return Err(McpError::Unauthorized { challenge });
            }
            let legacy_session = {
                let st = self.state();
                st.era == Some(McpEra::Legacy) && st.session.is_some()
            };
            if status == reqwest::StatusCode::NOT_FOUND && legacy_session {
                self.state().session = None;
                self.log.oxide("the server ended the session (HTTP 404)");
                self.closed_tx
                    .send_replace(Some(" (the session ended)".into()));
                return Err(McpError::SessionExpired);
            }
            if !status.is_success() {
                return Err(http_error(resp).await);
            }
            if method == "initialize"
                && let Some(s) = resp
                    .headers()
                    .get("mcp-session-id")
                    .and_then(|v| v.to_str().ok())
            {
                self.state().session = Some(s.to_owned());
            }
            if status == reqwest::StatusCode::ACCEPTED {
                return Err(McpError::Protocol(
                    "the server accepted the request without answering it".into(),
                ));
            }
            return if is_sse(&resp) {
                self.read_sse_response(resp, id).await
            } else {
                let text = resp.text().await?;
                self.read_json_response(&text, id)
            };
        }
    }

    fn read_json_response(&self, text: &str, id: &RequestId) -> Result<Value, McpError> {
        let msgs = jsonrpc::parse_text(text)
            .map_err(|_| McpError::Protocol(format!("not JSON: {}", snippet(text))))?;
        let mut answer = None;
        for msg in msgs {
            match msg {
                Incoming::Response {
                    id: Some(rid),
                    outcome,
                } if &rid == id => answer = Some(outcome),
                other => self.dispatch(other),
            }
        }
        match answer {
            Some(Ok(v)) => Ok(v),
            Some(Err(e)) => Err(McpError::Rpc(e)),
            None => Err(McpError::Protocol("the answer had no response".into())),
        }
    }

    async fn read_sse_response(
        &self,
        mut resp: reqwest::Response,
        id: &RequestId,
    ) -> Result<Value, McpError> {
        let mut parser = SseParser::new();
        loop {
            let chunk = resp.chunk().await?;
            let ended = chunk.is_none();
            let events = match chunk {
                Some(bytes) => parser.push(&bytes),
                None => parser.finish().into_iter().collect(),
            };
            for ev in events {
                if ev.data.trim().is_empty() {
                    continue;
                }
                let Ok(msgs) = jsonrpc::parse_text(&ev.data) else {
                    self.log
                        .push(McpLogSource::Stderr, Some("sse"), &snippet(&ev.data));
                    continue;
                };
                for msg in msgs {
                    match msg {
                        Incoming::Response {
                            id: Some(rid),
                            outcome,
                        } if &rid == id => {
                            return outcome.map_err(McpError::Rpc);
                        }
                        other => self.dispatch(other),
                    }
                }
            }
            if ended {
                return Err(McpError::Closed(
                    " (the response stream ended without an answer)".into(),
                ));
            }
        }
    }

    fn dispatch(&self, msg: Incoming) {
        match msg {
            Incoming::Request { id, method, params } => {
                let _ = self
                    .inbound
                    .send(ServerMessage::Request { id, method, params });
            }
            Incoming::Notification { method, params } => {
                let _ = self
                    .inbound
                    .send(ServerMessage::Notification { method, params });
            }
            Incoming::Response { .. } => {}
        }
    }

    /// Notifications and responses: the server answers 202 (or any 2xx) without a body.
    async fn post_one_way(&self, method: &str, body: &Value) -> Result<(), McpError> {
        let resp = self
            .base_request(
                method,
                reqwest::Method::POST,
                "application/json, text/event-stream",
            )
            .await?
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body.to_string())
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(())
        } else {
            Err(http_error(resp).await)
        }
    }

    async fn get_stream_loop(&self) {
        let mut delay = Duration::from_secs(1);
        loop {
            if self.closed_tx.borrow().is_some() {
                return;
            }
            let req = match self
                .base_request("", reqwest::Method::GET, "text/event-stream")
                .await
            {
                Ok(r) => r,
                Err(_) => return,
            };
            match req.send().await {
                Err(_) => {
                    tokio::time::sleep(delay).await;
                    delay = (delay * 2).min(Duration::from_secs(30));
                    continue;
                }
                Ok(resp) if !resp.status().is_success() || !is_sse(&resp) => {
                    // 405: this server has no standalone stream. That's fine.
                    tracing::debug!("MCP GET stream unavailable: {}", resp.status());
                    return;
                }
                Ok(mut resp) => {
                    delay = Duration::from_secs(1);
                    let mut parser = SseParser::new();
                    while let Ok(Some(bytes)) = resp.chunk().await {
                        for ev in parser.push(&bytes) {
                            if let Ok(msgs) = jsonrpc::parse_text(&ev.data) {
                                for m in msgs {
                                    self.dispatch(m);
                                }
                            }
                        }
                    }
                }
            }
            tokio::time::sleep(delay).await;
        }
    }
}

fn is_sse(resp: &reqwest::Response) -> bool {
    resp.headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ct| ct.to_ascii_lowercase().starts_with("text/event-stream"))
}

fn www_authenticate(resp: &reqwest::Response) -> Option<String> {
    resp.headers()
        .get(reqwest::header::WWW_AUTHENTICATE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
}

/// 403 with `error="insufficient_scope"`: the token needs more scopes (step-up sign-in).
fn is_scope_challenge(resp: &reqwest::Response) -> bool {
    resp.status() == reqwest::StatusCode::FORBIDDEN
        && www_authenticate(resp).is_some_and(|h| h.contains("insufficient_scope"))
}

async fn http_error(resp: reqwest::Response) -> McpError {
    let status = resp.status().as_u16();
    let text = resp.text().await.unwrap_or_default();
    let rpc = serde_json::from_str::<Value>(&text)
        .ok()
        .and_then(|v| v.get("error").cloned())
        .and_then(|e| serde_json::from_value::<RpcError>(e).ok());
    let detail = match &rpc {
        Some(e) => e.message.clone(),
        None => snippet(&text),
    };
    McpError::Http {
        status,
        detail,
        rpc,
    }
}

fn snippet(text: &str) -> String {
    let t = text.trim();
    if t.chars().count() > MAX_ERROR_BODY {
        let cut: String = t.chars().take(MAX_ERROR_BODY).collect();
        format!("{cut}…")
    } else {
        t.to_owned()
    }
}
