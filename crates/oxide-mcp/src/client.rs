//! A connected MCP server: era negotiation, requests with cancellation and multi round-trip
//! input, change subscriptions, and the typed operations the app uses.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use oxide_core::{
    McpEra, McpPrompt, McpPromptResult, McpResource, McpResourceRead, McpResourceTemplate, McpTool,
    McpToolResult,
};
use serde_json::{Map, Value, json};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;

use crate::McpError;
use crate::handler::{ClientEvent, ClientHandler};
use crate::jsonrpc::{
    INVALID_PARAMS, METHOD_NOT_FOUND, RequestId, RpcError, UNSUPPORTED_PROTOCOL_VERSION,
};
use crate::protocol::{
    self, LEGACY_VERSIONS, META_SERVER_INFO, MODERN_VERSIONS, client_capabilities, client_info,
};
use crate::transport::{RequestOptions, ServerMessage, Transport};

/// Most pages read from one list.
const MAX_PAGES: usize = 100;
/// Most input rounds for one request before giving up.
const MAX_INPUT_ROUNDS: usize = 6;

/// How to connect.
#[derive(Debug, Clone)]
pub struct ConnectOptions {
    /// Skip the modern probe (a cached result for this server).
    pub prefer: Option<McpEra>,
    /// How long the stdio `server/discover` probe waits before assuming a legacy server.
    pub probe_timeout: Duration,
    /// Timeout of ordinary requests (handshake, lists, reads).
    pub request_timeout: Duration,
    /// Timeout of tool calls.
    pub tool_timeout: Duration,
}

impl Default for ConnectOptions {
    fn default() -> Self {
        Self {
            prefer: None,
            probe_timeout: Duration::from_secs(20),
            request_timeout: Duration::from_secs(60),
            tool_timeout: Duration::from_secs(600),
        }
    }
}

/// What the server told us about itself.
#[derive(Debug, Clone, PartialEq)]
pub struct ServerInfo {
    pub era: McpEra,
    pub version: String,
    pub name: Option<String>,
    pub server_version: Option<String>,
    pub instructions: Option<String>,
    pub capabilities: Value,
}

struct Shared {
    name: String,
    transport: Transport,
    handler: Arc<dyn ClientHandler>,
    next_id: AtomicI64,
}

impl Shared {
    fn id(&self) -> RequestId {
        RequestId::Num(self.next_id.fetch_add(1, Ordering::Relaxed))
    }

    /// Answers a server-to-client request.
    async fn serve(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        match method {
            "ping" => Ok(json!({})),
            "roots/list" => Ok(json!({"roots": self.handler.roots(&self.name).await})),
            "sampling/createMessage" => self.handler.sample(&self.name, params).await,
            "elicitation/create" => self.handler.elicit(&self.name, params).await,
            other => Err(RpcError::new(
                METHOD_NOT_FOUND,
                format!("Oxide doesn't support {other}"),
            )),
        }
    }
}

pub struct McpClient {
    shared: Arc<Shared>,
    info: ServerInfo,
    timeouts: ConnectOptions,
    /// Input schemas by tool name (for `Mcp-Param-*` headers).
    tool_schemas: Mutex<HashMap<String, Value>>,
    tasks: Mutex<Vec<JoinHandle<()>>>,
}

/// Sends `notifications/cancelled` if a request ends without its answer.
struct CancelGuard {
    shared: Arc<Shared>,
    id: RequestId,
    armed: bool,
}

impl Drop for CancelGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let Ok(rt) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let shared = self.shared.clone();
        let id = self.id.clone();
        rt.spawn(async move {
            let _ = shared
                .transport
                .notify(
                    "notifications/cancelled",
                    json!({"requestId": Value::from(id), "reason": "cancelled by the user"}),
                )
                .await;
        });
    }
}

impl McpClient {
    /// Negotiates with the server behind `transport`. `inbound` is the receiving end of the
    /// channel the transport was created with.
    pub async fn connect(
        name: &str,
        transport: Transport,
        inbound: mpsc::UnboundedReceiver<ServerMessage>,
        handler: Arc<dyn ClientHandler>,
        opts: ConnectOptions,
    ) -> Result<Arc<Self>, McpError> {
        let shared = Arc::new(Shared {
            name: name.to_owned(),
            transport,
            handler,
            next_id: AtomicI64::new(1),
        });
        let inbound_task = tokio::spawn(inbound_loop(shared.clone(), inbound));
        let negotiated = negotiate(&shared, &opts).await;
        let info = match negotiated {
            Ok(info) => info,
            Err(e) => {
                inbound_task.abort();
                shared.transport.close().await;
                return Err(e);
            }
        };
        let client = Arc::new(Self {
            shared,
            info,
            timeouts: opts,
            tool_schemas: Mutex::new(HashMap::new()),
            tasks: Mutex::new(vec![inbound_task]),
        });
        if client.info.era == McpEra::Modern {
            client.start_subscription();
        }
        Ok(client)
    }

    pub fn name(&self) -> &str {
        &self.shared.name
    }

    pub fn info(&self) -> &ServerInfo {
        &self.info
    }

    /// Becomes `Some(reason)` when the connection is gone.
    pub fn closed(&self) -> watch::Receiver<Option<String>> {
        self.shared.transport.closed()
    }

    pub async fn close(&self) {
        for t in self
            .tasks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .drain(..)
        {
            t.abort();
        }
        self.shared.transport.close().await;
    }

    fn has(&self, capability: &str) -> bool {
        self.info.capabilities.get(capability).is_some()
    }

    /// One request with the era's metadata.
    async fn call(
        &self,
        method: &str,
        params: Value,
        progress: Option<&str>,
        timeout: Option<Duration>,
        headers: Vec<(String, String)>,
    ) -> Result<Value, McpError> {
        request(
            &self.shared,
            &self.info,
            method,
            params,
            progress,
            RequestOptions { timeout, headers },
        )
        .await
    }

    /// A request that may come back with `input_required` (tools/call, resources/read,
    /// prompts/get): gathers the input and retries until the server completes it.
    async fn call_with_input(
        &self,
        method: &str,
        params: Value,
        progress: Option<&str>,
        timeout: Duration,
        headers: Vec<(String, String)>,
    ) -> Result<Value, McpError> {
        let mut params = params;
        for _ in 0..MAX_INPUT_ROUNDS {
            let res = self
                .call(
                    method,
                    params.clone(),
                    progress,
                    Some(timeout),
                    headers.clone(),
                )
                .await?;
            match res.get("resultType").and_then(Value::as_str) {
                None | Some("complete") => return Ok(res),
                Some("input_required") => {}
                Some(other) => {
                    return Err(McpError::Protocol(format!(
                        "unknown resultType \"{other}\""
                    )));
                }
            }
            let mut responses = Map::new();
            if let Some(reqs) = res.get("inputRequests").and_then(Value::as_object) {
                for (key, req) in reqs {
                    let m = req.get("method").and_then(Value::as_str).unwrap_or("");
                    let p = req.get("params").cloned().unwrap_or(Value::Null);
                    match self.shared.serve(m, p).await {
                        Ok(v) => {
                            responses.insert(key.clone(), v);
                        }
                        Err(e) => return Err(McpError::Rpc(e)),
                    }
                }
            }
            let mut obj = match params {
                Value::Object(m) => m,
                _ => Map::new(),
            };
            obj.remove("inputResponses");
            obj.remove("requestState");
            if !responses.is_empty() {
                obj.insert("inputResponses".into(), Value::Object(responses));
            }
            if let Some(state) = res.get("requestState").filter(|s| !s.is_null()) {
                obj.insert("requestState".into(), state.clone());
            }
            params = Value::Object(obj);
        }
        Err(McpError::Protocol(
            "the server kept asking for more input".into(),
        ))
    }

    async fn list_all(&self, method: &str, key: &str) -> Result<Vec<Value>, McpError> {
        let mut out = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let params = match &cursor {
                Some(c) => json!({"cursor": c}),
                None => json!({}),
            };
            let res = self
                .call(
                    method,
                    params,
                    None,
                    Some(self.timeouts.request_timeout),
                    Vec::new(),
                )
                .await?;
            if let Some(items) = res.get(key).and_then(Value::as_array) {
                out.extend(items.iter().cloned());
            }
            cursor = res
                .get("nextCursor")
                .and_then(Value::as_str)
                .filter(|c| !c.is_empty())
                .map(str::to_owned);
            if cursor.is_none() {
                break;
            }
        }
        Ok(out)
    }

    pub async fn list_tools(&self) -> Result<Vec<McpTool>, McpError> {
        if !self.has("tools") {
            return Ok(Vec::new());
        }
        let raw = self.list_all("tools/list", "tools").await?;
        let http = self.shared.transport.is_http();
        let mut schemas = HashMap::new();
        let mut tools = Vec::new();
        for t in raw {
            let schema = t.get("inputSchema").cloned().unwrap_or(Value::Null);
            if http && let Err(why) = protocol::header_params(&schema) {
                let name = t.get("name").and_then(Value::as_str).unwrap_or("?");
                tracing::warn!("MCP server {}: hiding tool {name}: {why}", self.name());
                continue;
            }
            if let Some(tool) = protocol::parse_tool(self.name(), &t) {
                schemas.insert(tool.name.clone(), schema);
                tools.push(tool);
            }
        }
        *self.tool_schemas.lock().unwrap_or_else(|e| e.into_inner()) = schemas;
        Ok(tools)
    }

    pub async fn list_resources(&self) -> Result<Vec<McpResource>, McpError> {
        if !self.has("resources") {
            return Ok(Vec::new());
        }
        let raw = self.list_all("resources/list", "resources").await?;
        Ok(raw
            .iter()
            .filter_map(|r| protocol::parse_resource(self.name(), r))
            .collect())
    }

    pub async fn list_resource_templates(&self) -> Result<Vec<McpResourceTemplate>, McpError> {
        if !self.has("resources") {
            return Ok(Vec::new());
        }
        let raw = match self
            .list_all("resources/templates/list", "resourceTemplates")
            .await
        {
            Ok(r) => r,
            // Optional for servers; some answer "method not found".
            Err(e) if e.rpc().is_some_and(|r| r.code == METHOD_NOT_FOUND) => Vec::new(),
            Err(e) => return Err(e),
        };
        Ok(raw
            .iter()
            .filter_map(|r| protocol::parse_template(self.name(), r))
            .collect())
    }

    pub async fn list_prompts(&self) -> Result<Vec<McpPrompt>, McpError> {
        if !self.has("prompts") {
            return Ok(Vec::new());
        }
        let raw = self.list_all("prompts/list", "prompts").await?;
        Ok(raw
            .iter()
            .filter_map(|p| protocol::parse_prompt(self.name(), p))
            .collect())
    }

    /// Runs a tool. `progress` is a token for `notifications/progress` (the UI's call id).
    pub async fn call_tool(
        &self,
        tool: &str,
        arguments: Value,
        progress: Option<&str>,
    ) -> Result<McpToolResult, McpError> {
        let headers = if self.shared.transport.is_http() && self.info.era == McpEra::Modern {
            let schema = self
                .tool_schemas
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get(tool)
                .cloned()
                .unwrap_or(Value::Null);
            protocol::param_headers(&schema, &arguments)
        } else {
            Vec::new()
        };
        let started = Instant::now();
        let res = self
            .call_with_input(
                "tools/call",
                json!({"name": tool, "arguments": arguments}),
                progress,
                self.timeouts.tool_timeout,
                headers,
            )
            .await?;
        let ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        Ok(protocol::parse_tool_result(&res, ms))
    }

    pub async fn read_resource(&self, uri: &str) -> Result<McpResourceRead, McpError> {
        let res = self
            .call_with_input(
                "resources/read",
                json!({"uri": uri}),
                None,
                self.timeouts.request_timeout,
                Vec::new(),
            )
            .await?;
        Ok(protocol::parse_resource_read(&res))
    }

    pub async fn get_prompt(
        &self,
        prompt: &str,
        arguments: &Map<String, Value>,
    ) -> Result<McpPromptResult, McpError> {
        let res = self
            .call_with_input(
                "prompts/get",
                json!({"name": prompt, "arguments": arguments}),
                None,
                self.timeouts.request_timeout,
                Vec::new(),
            )
            .await?;
        Ok(protocol::parse_prompt_result(&res))
    }

    pub async fn ping(&self) -> Result<(), McpError> {
        self.call(
            "ping",
            json!({}),
            None,
            Some(Duration::from_secs(10)),
            Vec::new(),
        )
        .await
        .map(|_| ())
    }

    /// Modern servers: a long-lived `subscriptions/listen` for list changes.
    fn start_subscription(self: &Arc<Self>) {
        let caps = &self.info.capabilities;
        let changed = |k: &str| {
            caps.get(k)
                .and_then(|c| c.get("listChanged"))
                .and_then(Value::as_bool)
                .unwrap_or(false)
        };
        let filter = json!({
            "toolsListChanged": changed("tools"),
            "promptsListChanged": changed("prompts"),
            "resourcesListChanged": changed("resources"),
        });
        if !(changed("tools") || changed("prompts") || changed("resources")) {
            return;
        }
        let shared = self.shared.clone();
        let info = self.info.clone();
        let task = tokio::spawn(async move {
            let mut delay = Duration::from_secs(2);
            loop {
                let res = request(
                    &shared,
                    &info,
                    "subscriptions/listen",
                    json!({"notifications": filter}),
                    None,
                    RequestOptions::default(),
                )
                .await;
                match res {
                    // The server ended it gracefully; listen again.
                    Ok(_) => delay = Duration::from_secs(2),
                    Err(McpError::Closed(_)) | Err(McpError::Cancelled) => return,
                    Err(e)
                        if e.rpc().is_some_and(|r| {
                            r.code == METHOD_NOT_FOUND || r.code == INVALID_PARAMS
                        }) =>
                    {
                        return;
                    }
                    Err(e) => {
                        tracing::debug!("MCP subscription for {} failed: {e}", shared.name);
                        delay = (delay * 2).min(Duration::from_secs(60));
                    }
                }
                tokio::time::sleep(delay).await;
            }
        });
        self.tasks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(task);
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        for t in self
            .tasks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .drain(..)
        {
            t.abort();
        }
    }
}

/// Sends one request with the era's metadata. Cancels it on the server if the caller gives
/// up (drops the future or times out).
async fn request(
    shared: &Arc<Shared>,
    info: &ServerInfo,
    method: &str,
    params: Value,
    progress: Option<&str>,
    opts: RequestOptions,
) -> Result<Value, McpError> {
    let id = shared.id();
    let params = match info.era {
        McpEra::Modern => protocol::with_modern_meta(params, &info.version, progress),
        McpEra::Legacy => protocol::with_progress_token(params, progress),
    };
    let mut guard = CancelGuard {
        shared: shared.clone(),
        id: id.clone(),
        armed: shared.transport.sends_cancel_notifications(info.era),
    };
    let res = shared.transport.request(&id, method, params, &opts).await;
    if !matches!(res, Err(McpError::Timeout(_))) {
        guard.armed = false;
    }
    res
}

async fn negotiate(shared: &Arc<Shared>, opts: &ConnectOptions) -> Result<ServerInfo, McpError> {
    if opts.prefer != Some(McpEra::Legacy)
        && let Some(info) = probe_modern(shared, opts).await?
    {
        return Ok(info);
    }
    initialize_legacy(shared, opts).await
}

/// `server/discover`. `Ok(None)`: not a modern server, fall back to `initialize`.
async fn probe_modern(
    shared: &Arc<Shared>,
    opts: &ConnectOptions,
) -> Result<Option<ServerInfo>, McpError> {
    let mut version = MODERN_VERSIONS[0].to_owned();
    for _ in 0..2 {
        shared.transport.set_protocol(McpEra::Modern, &version);
        let probe_info = ServerInfo {
            era: McpEra::Modern,
            version: version.clone(),
            name: None,
            server_version: None,
            instructions: None,
            capabilities: Value::Null,
        };
        let timeout = if shared.transport.is_http() {
            opts.request_timeout
        } else {
            opts.probe_timeout
        };
        let res = request(
            shared,
            &probe_info,
            "server/discover",
            json!({}),
            None,
            RequestOptions {
                timeout: Some(timeout),
                headers: Vec::new(),
            },
        )
        .await;
        match res {
            Ok(r) => {
                let supported: Vec<String> = r
                    .get("supportedVersions")
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .filter_map(Value::as_str)
                            .map(str::to_owned)
                            .collect()
                    })
                    .unwrap_or_default();
                if supported.is_empty() || supported.contains(&version) {
                    return Ok(Some(discover_info(&r, &version)));
                }
                if let Some(v) = pick(&supported, MODERN_VERSIONS) {
                    version = v;
                    continue;
                }
                if pick(&supported, LEGACY_VERSIONS).is_some() {
                    return Ok(None);
                }
                return Err(unsupported(&supported));
            }
            Err(e) => {
                if let Some(rpc) = e.rpc().filter(|r| r.is_modern()) {
                    if rpc.code == UNSUPPORTED_PROTOCOL_VERSION {
                        let supported = rpc.supported_versions();
                        if let Some(v) = pick(&supported, MODERN_VERSIONS)
                            && v != version
                        {
                            version = v;
                            continue;
                        }
                        if pick(&supported, LEGACY_VERSIONS).is_some() {
                            return Ok(None);
                        }
                        return Err(unsupported(&supported));
                    }
                    // A modern server that refused the probe for another reason.
                    return Err(e);
                }
                match e {
                    McpError::Unauthorized { .. }
                    | McpError::Network(_)
                    | McpError::Config(_)
                    | McpError::Spawn(_) => return Err(e),
                    // The process died on an unknown method: a legacy server that crashes
                    // before `initialize`. The manager retries it as legacy.
                    McpError::Closed(_) => return Err(e),
                    // Anything else (timeout, non-modern error, HTTP 4xx): legacy.
                    _ => return Ok(None),
                }
            }
        }
    }
    Ok(None)
}

fn pick(supported: &[String], ours: &[&str]) -> Option<String> {
    ours.iter()
        .find(|v| supported.iter().any(|s| s == *v))
        .map(|v| (*v).to_owned())
}

fn unsupported(supported: &[String]) -> McpError {
    McpError::Unsupported(format!(
        "the server only speaks protocol versions {} (Oxide speaks {} and {})",
        supported.join(", "),
        MODERN_VERSIONS.join(", "),
        LEGACY_VERSIONS.join(", ")
    ))
}

fn discover_info(r: &Value, version: &str) -> ServerInfo {
    let server = r
        .get("_meta")
        .and_then(|m| m.get(META_SERVER_INFO))
        .or_else(|| r.get("serverInfo"));
    ServerInfo {
        era: McpEra::Modern,
        version: version.to_owned(),
        name: server
            .and_then(|s| s.get("name"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        server_version: server
            .and_then(|s| s.get("version"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        instructions: r
            .get("instructions")
            .and_then(Value::as_str)
            .map(str::to_owned),
        capabilities: r.get("capabilities").cloned().unwrap_or_else(|| json!({})),
    }
}

async fn initialize_legacy(
    shared: &Arc<Shared>,
    opts: &ConnectOptions,
) -> Result<ServerInfo, McpError> {
    shared.transport.set_protocol(McpEra::Legacy, "");
    let pre = ServerInfo {
        era: McpEra::Legacy,
        version: String::new(),
        name: None,
        server_version: None,
        instructions: None,
        capabilities: Value::Null,
    };
    let res = request(
        shared,
        &pre,
        "initialize",
        json!({
            "protocolVersion": LEGACY_VERSIONS[0],
            "capabilities": client_capabilities(),
            "clientInfo": client_info(),
        }),
        None,
        RequestOptions {
            timeout: Some(opts.request_timeout),
            headers: Vec::new(),
        },
    )
    .await
    .map_err(|e| match e {
        // 404/405 on POST: the old HTTP+SSE transport (2024-11-05).
        McpError::Http {
            status: 404 | 405, ..
        } if shared.transport.is_http() => McpError::Unsupported(
            "the server doesn't accept Streamable HTTP. If it uses the old HTTP+SSE transport \
             (a URL ending in /sse), use its Streamable HTTP endpoint instead (often /mcp)."
                .into(),
        ),
        other => other,
    })?;
    let version = res
        .get("protocolVersion")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    if !LEGACY_VERSIONS.contains(&version.as_str()) {
        return Err(McpError::Unsupported(format!(
            "the server wants protocol version \"{version}\", which Oxide doesn't speak"
        )));
    }
    shared.transport.set_protocol(McpEra::Legacy, &version);
    let server = res.get("serverInfo");
    let info = ServerInfo {
        era: McpEra::Legacy,
        version,
        name: server
            .and_then(|s| s.get("name"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        server_version: server
            .and_then(|s| s.get("version"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        instructions: res
            .get("instructions")
            .and_then(Value::as_str)
            .map(str::to_owned),
        capabilities: res
            .get("capabilities")
            .cloned()
            .unwrap_or_else(|| json!({})),
    };
    shared
        .transport
        .notify("notifications/initialized", json!({}))
        .await?;
    if info.capabilities.get("logging").is_some() {
        let _ = request(
            shared,
            &info,
            "logging/setLevel",
            json!({"level": "info"}),
            None,
            RequestOptions {
                timeout: Some(Duration::from_secs(10)),
                headers: Vec::new(),
            },
        )
        .await;
    }
    shared.transport.start_listening();
    Ok(info)
}

async fn inbound_loop(shared: Arc<Shared>, mut rx: mpsc::UnboundedReceiver<ServerMessage>) {
    while let Some(msg) = rx.recv().await {
        match msg {
            ServerMessage::Notification { method, params } => {
                if let Some(ev) = notification_event(&method, &params) {
                    shared.handler.event(&shared.name, ev);
                }
            }
            ServerMessage::Request { id, method, params } => {
                let shared = shared.clone();
                tokio::spawn(async move {
                    let outcome = shared.serve(&method, params).await;
                    if let Err(e) = shared.transport.respond(&id, outcome).await {
                        tracing::debug!("couldn't answer {method} from {}: {e}", shared.name);
                    }
                });
            }
        }
    }
}

fn notification_event(method: &str, params: &Value) -> Option<ClientEvent> {
    match method {
        "notifications/message" => {
            let level = params
                .get("level")
                .and_then(Value::as_str)
                .unwrap_or("info")
                .to_owned();
            let data = params.get("data").cloned().unwrap_or(Value::Null);
            let mut text = match data {
                Value::String(s) => s,
                other => other.to_string(),
            };
            if let Some(logger) = params.get("logger").and_then(Value::as_str) {
                text = format!("{logger}: {text}");
            }
            Some(ClientEvent::Log { level, text })
        }
        "notifications/progress" => {
            let token = match params.get("progressToken")? {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            Some(ClientEvent::Progress {
                token,
                progress: params
                    .get("progress")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0),
                total: params.get("total").and_then(Value::as_f64),
                message: params
                    .get("message")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
            })
        }
        "notifications/tools/list_changed"
        | "notifications/resources/list_changed"
        | "notifications/prompts/list_changed" => Some(ClientEvent::ListChanged),
        "notifications/resources/updated" => Some(ClientEvent::ResourceUpdated {
            uri: params.get("uri").and_then(Value::as_str)?.to_owned(),
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notifications_become_events() {
        assert_eq!(
            notification_event(
                "notifications/message",
                &json!({"level": "warning", "logger": "db", "data": {"x": 1}})
            ),
            Some(ClientEvent::Log {
                level: "warning".into(),
                text: r#"db: {"x":1}"#.into()
            })
        );
        assert_eq!(
            notification_event(
                "notifications/progress",
                &json!({"progressToken": "t1", "progress": 2, "total": 4})
            ),
            Some(ClientEvent::Progress {
                token: "t1".into(),
                progress: 2.0,
                total: Some(4.0),
                message: None
            })
        );
        assert_eq!(
            notification_event("notifications/tools/list_changed", &json!({})),
            Some(ClientEvent::ListChanged)
        );
        assert_eq!(notification_event("notifications/other", &json!({})), None);
    }

    #[test]
    fn versions_are_picked_from_ours() {
        let s = vec!["2025-06-18".to_string(), "1999-01-01".to_string()];
        assert_eq!(pick(&s, LEGACY_VERSIONS).as_deref(), Some("2025-06-18"));
        assert_eq!(pick(&s, MODERN_VERSIONS), None);
    }
}
