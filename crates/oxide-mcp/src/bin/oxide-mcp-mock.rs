//! A tiny stdio MCP server for Oxide's tests (`tests/stdio.rs`). Not shipped.
//!
//! `oxide-mcp-mock <mode>` where mode is:
//! - `modern`: 2026-07-28 only (rejects `initialize`).
//! - `legacy`: 2025-06-18 only; answers unknown methods before `initialize` with an error.
//! - `legacy-crash`: like `legacy`, but exits on any request before `initialize`.
//! - `legacy-silent`: like `legacy`, but ignores requests before `initialize`.
//!
//! Tools: `echo {text}`, `add {a, b}`, `slow {steps}` (progress, honours cancellation),
//! `ask` (elicitation), `sample` (sampling), `roots`. Resource `mock://readme`, prompt `greet`.

use std::collections::{HashMap, HashSet};
use std::io::{BufRead, Write};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use serde_json::{Value, json};

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Modern,
    Legacy,
    LegacyCrash,
    LegacySilent,
}

struct Server {
    mode: Mode,
    out: Mutex<std::io::Stdout>,
    initialized: Mutex<bool>,
    cancelled: Mutex<HashSet<String>>,
    /// Responses to our own requests (legacy), by id.
    replies: Mutex<HashMap<String, Value>>,
    reply_ready: Condvar,
    next: Mutex<u64>,
}

impl Server {
    fn send(&self, v: &Value) {
        let mut out = self.out.lock().unwrap_or_else(|e| e.into_inner());
        let _ = writeln!(out, "{v}");
        let _ = out.flush();
    }

    fn result(&self, id: &Value, result: Value) {
        self.send(&json!({"jsonrpc": "2.0", "id": id, "result": result}));
    }

    fn error(&self, id: &Value, code: i64, message: &str, data: Option<Value>) {
        let mut err = json!({"code": code, "message": message});
        if let Some(d) = data {
            err["data"] = d;
        }
        self.send(&json!({"jsonrpc": "2.0", "id": id, "error": err}));
    }

    /// Legacy only: asks the client something and waits for the answer.
    fn ask_client(&self, method: &str, params: Value) -> Value {
        let id = {
            let mut n = self.next.lock().unwrap_or_else(|e| e.into_inner());
            *n += 1;
            format!("srv-{n}")
        };
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        let mut replies = self.replies.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if let Some(v) = replies.remove(&id) {
                return v;
            }
            replies = self
                .reply_ready
                .wait(replies)
                .unwrap_or_else(|e| e.into_inner());
        }
    }

    fn modern(&self) -> bool {
        self.mode == Mode::Modern
    }

    fn capabilities(&self) -> Value {
        let mut caps = json!({
            "tools": {"listChanged": true},
            "resources": {},
            "prompts": {},
        });
        if !self.modern() {
            caps["logging"] = json!({});
        }
        caps
    }

    fn handle(self: &Arc<Self>, msg: Value) {
        let Some(method) = msg.get("method").and_then(Value::as_str).map(str::to_owned) else {
            // A response to one of our requests.
            if let Some(id) = msg.get("id").and_then(Value::as_str) {
                let mut replies = self.replies.lock().unwrap_or_else(|e| e.into_inner());
                replies.insert(id.to_owned(), msg.clone());
                self.reply_ready.notify_all();
            }
            return;
        };
        let params = msg.get("params").cloned().unwrap_or(Value::Null);
        let Some(id) = msg.get("id").cloned() else {
            if method == "notifications/initialized" {
                *self.initialized.lock().unwrap_or_else(|e| e.into_inner()) = true;
            }
            if method == "notifications/cancelled"
                && let Some(r) = params.get("requestId")
            {
                self.cancelled
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(r.to_string());
            }
            return;
        };

        if self.modern() {
            if method == "initialize" {
                return self.error(
                    &id,
                    -32601,
                    "initialize isn't supported; use 2026-07-28",
                    None,
                );
            }
            let version = params
                .pointer("/_meta/io.modelcontextprotocol~1protocolVersion")
                .and_then(Value::as_str);
            match version {
                Some("2026-07-28") => {}
                Some(v) => {
                    return self.error(
                        &id,
                        -32022,
                        "Unsupported protocol version",
                        Some(json!({"supported": ["2026-07-28"], "requested": v})),
                    );
                }
                None => return self.error(&id, -32602, "missing _meta", None),
            }
        } else if method != "initialize"
            && method != "ping"
            && !*self.initialized.lock().unwrap_or_else(|e| e.into_inner())
        {
            match self.mode {
                Mode::LegacyCrash => std::process::exit(3),
                Mode::LegacySilent => return,
                _ => return self.error(&id, -32601, "Method not found", None),
            }
        }

        match method.as_str() {
            "server/discover" => self.result(
                &id,
                json!({
                    "resultType": "complete",
                    "supportedVersions": ["2026-07-28"],
                    "capabilities": self.capabilities(),
                    "_meta": {"io.modelcontextprotocol/serverInfo": {"name": "mock", "version": "1.0"}},
                    "instructions": "A test server."
                }),
            ),
            "initialize" => self.result(
                &id,
                json!({
                    "protocolVersion": "2025-06-18",
                    "capabilities": self.capabilities(),
                    "serverInfo": {"name": "mock-legacy", "version": "0.9"}
                }),
            ),
            "ping" => self.result(&id, json!({})),
            "logging/setLevel" => self.result(&id, json!({})),
            "subscriptions/listen" => {
                // Acknowledge, then announce a change once.
                let sub = json!({"io.modelcontextprotocol/subscriptionId": id});
                self.send(&json!({"jsonrpc": "2.0", "method": "notifications/subscriptions/acknowledged",
                                  "params": {"_meta": sub, "notifications": params["notifications"]}}));
                self.send(&json!({"jsonrpc": "2.0", "method": "notifications/tools/list_changed",
                                  "params": {"_meta": sub}}));
            }
            "tools/list" => {
                // Two pages, to test cursors.
                let cursor = params.get("cursor").and_then(Value::as_str);
                let tools = if cursor.is_none() {
                    json!({"tools": [
                        {"name": "echo", "description": "Echoes text", "inputSchema": {"type": "object", "properties": {"text": {"type": "string"}}}},
                        {"name": "add", "inputSchema": {"type": "object"}, "annotations": {"readOnlyHint": true}},
                        {"name": "slow", "inputSchema": {"type": "object"}}
                    ], "nextCursor": "p2"})
                } else {
                    json!({"tools": [
                        {"name": "ask", "inputSchema": {"type": "object"}},
                        {"name": "sample", "inputSchema": {"type": "object"}},
                        {"name": "roots", "inputSchema": {"type": "object"}}
                    ]})
                };
                self.result(&id, tools);
            }
            "resources/list" => self.result(
                &id,
                json!({"resources": [{"uri": "mock://readme", "name": "readme", "mimeType": "text/plain"}]}),
            ),
            "resources/templates/list" => self.result(&id, json!({"resourceTemplates": []})),
            "resources/read" => self.result(
                &id,
                json!({"contents": [{"uri": params["uri"], "mimeType": "text/plain", "text": "Read me."}]}),
            ),
            "prompts/list" => self.result(
                &id,
                json!({"prompts": [{"name": "greet", "arguments": [{"name": "name", "required": true}]}]}),
            ),
            "prompts/get" => {
                let name = params
                    .pointer("/arguments/name")
                    .and_then(Value::as_str)
                    .unwrap_or("you");
                self.result(
                    &id,
                    json!({"messages": [{"role": "user", "content": {"type": "text", "text": format!("Say hello to {name}.")}}]}),
                );
            }
            "tools/call" => {
                let this = self.clone();
                std::thread::spawn(move || this.call_tool(&id, &params));
            }
            _ => self.error(&id, -32601, "Method not found", None),
        }
    }

    fn call_tool(&self, id: &Value, params: &Value) {
        let name = params.get("name").and_then(Value::as_str).unwrap_or("");
        let args = params.get("arguments").cloned().unwrap_or(json!({}));
        let text = |t: String| json!({"content": [{"type": "text", "text": t}]});
        match name {
            "echo" => self.result(id, text(args["text"].as_str().unwrap_or("").to_owned())),
            "add" => {
                let sum = args["a"].as_f64().unwrap_or(0.0) + args["b"].as_f64().unwrap_or(0.0);
                self.result(
                    id,
                    json!({"content": [{"type": "text", "text": sum.to_string()}], "structuredContent": {"sum": sum}}),
                );
            }
            "slow" => {
                let steps = args["steps"].as_u64().unwrap_or(3);
                let token = params.pointer("/_meta/progressToken").cloned();
                for i in 1..=steps {
                    std::thread::sleep(Duration::from_millis(100));
                    if self
                        .cancelled
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .contains(&id.to_string())
                    {
                        eprintln!("cancelled {id}");
                        return;
                    }
                    if let Some(t) = &token {
                        self.send(&json!({"jsonrpc": "2.0", "method": "notifications/progress",
                            "params": {"progressToken": t, "progress": i, "total": steps, "message": format!("step {i}")}}));
                    }
                }
                self.result(id, text("done".into()));
            }
            "ask" => {
                let schema = json!({"type": "object", "properties": {"color": {"type": "string"}}, "required": ["color"]});
                let answer = if self.modern() {
                    match params.get("inputResponses").and_then(|r| r.get("color")) {
                        Some(a) => a.clone(),
                        None => {
                            return self.result(
                                id,
                                json!({"resultType": "input_required",
                                       "inputRequests": {"color": {"method": "elicitation/create",
                                           "params": {"mode": "form", "message": "Favourite colour?", "requestedSchema": schema}}},
                                       "requestState": "state-1"}),
                            );
                        }
                    }
                } else {
                    self.send(&json!({"jsonrpc": "2.0", "method": "notifications/message",
                        "params": {"level": "info", "data": "asking for a colour"}}));
                    let reply = self.ask_client(
                        "elicitation/create",
                        json!({"message": "Favourite colour?", "requestedSchema": schema}),
                    );
                    reply.get("result").cloned().unwrap_or(Value::Null)
                };
                let state = params.get("requestState").cloned().unwrap_or(Value::Null);
                self.result(
                    id,
                    text(format!(
                        "{} {} state={state}",
                        answer["action"].as_str().unwrap_or("?"),
                        answer
                            .pointer("/content/color")
                            .and_then(Value::as_str)
                            .unwrap_or("-")
                    )),
                );
            }
            "sample" => {
                let request = json!({"messages": [{"role": "user", "content": {"type": "text", "text": "Hi?"}}], "maxTokens": 10});
                let answer = if self.modern() {
                    match params.pointer("/inputResponses/s") {
                        Some(a) => a.clone(),
                        None => {
                            return self.result(
                                id,
                                json!({"resultType": "input_required", "inputRequests": {"s": {"method": "sampling/createMessage", "params": request}}}),
                            );
                        }
                    }
                } else {
                    let reply = self.ask_client("sampling/createMessage", request);
                    match reply.get("result") {
                        Some(r) => r.clone(),
                        None => return self.result(id, json!({"content": [{"type": "text", "text": "refused"}], "isError": true})),
                    }
                };
                self.result(
                    id,
                    text(
                        answer
                            .pointer("/content/text")
                            .and_then(Value::as_str)
                            .unwrap_or("?")
                            .to_owned(),
                    ),
                );
            }
            "roots" => {
                let roots = if self.modern() {
                    json!([])
                } else {
                    self.ask_client("roots/list", json!({}))["result"]["roots"].clone()
                };
                self.result(id, text(format!("roots={roots}")));
            }
            _ => self.result(
                id,
                json!({"content": [{"type": "text", "text": "unknown tool"}], "isError": true}),
            ),
        }
    }
}

fn main() {
    let mode = match std::env::args().nth(1).as_deref() {
        Some("legacy") => Mode::Legacy,
        Some("legacy-crash") => Mode::LegacyCrash,
        Some("legacy-silent") => Mode::LegacySilent,
        _ => Mode::Modern,
    };
    eprintln!("mock server starting");
    let server = Arc::new(Server {
        mode,
        out: Mutex::new(std::io::stdout()),
        initialized: Mutex::new(false),
        cancelled: Mutex::new(HashSet::new()),
        replies: Mutex::new(HashMap::new()),
        reply_ready: Condvar::new(),
        next: Mutex::new(0),
    });
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let Ok(msg) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        server.handle(msg);
    }
    eprintln!("mock server: stdin closed");
}
