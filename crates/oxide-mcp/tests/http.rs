//! Streamable HTTP against a scripted local server: modern headers, legacy sessions,
//! SSE answers, auth challenges and version fallback.

mod common;

use std::sync::Arc;

use common::{Reply, Req, TestServer, result, serve, status};

use oxide_core::McpEra;
use oxide_mcp::log::ServerLog;
use oxide_mcp::transport::{HttpConfig, HttpTransport, Transport};
use oxide_mcp::{ConnectOptions, McpClient, McpError, NoUser};
use serde_json::json;
use tokio::sync::mpsc;

async fn connect(server: &TestServer) -> Result<Arc<McpClient>, McpError> {
    let (tx, rx) = mpsc::unbounded_channel();
    let t = HttpTransport::new(
        &HttpConfig {
            url: server.url.clone(),
            headers: vec![("X-Api-Key".into(), "k".into())],
        },
        None,
        tx,
        ServerLog::memory(),
    )
    .expect("transport");
    McpClient::connect(
        "remote",
        Transport::Http(t),
        rx,
        Arc::new(NoUser),
        ConnectOptions::default(),
    )
    .await
}

#[tokio::test]
async fn modern_http_sends_metadata_headers_and_reads_sse() {
    let server = serve(Arc::new(|req: &Req| match req.rpc_method() {
        "server/discover" => result(
            req,
            json!({"resultType": "complete", "supportedVersions": ["2026-07-28"],
                   "capabilities": {"tools": {}},
                   "_meta": {"io.modelcontextprotocol/serverInfo": {"name": "remote", "version": "2"}}}),
        ),
        "tools/list" => result(
            req,
            json!({"tools": [
                {"name": "query", "inputSchema": {"type": "object", "properties": {
                    "region": {"type": "string", "x-mcp-header": "Region"}}}},
                {"name": "broken", "inputSchema": {"type": "object", "properties": {
                    "n": {"type": "number", "x-mcp-header": "N"}}}}
            ]}),
        ),
        "tools/call" => {
            let id = req.body["id"].clone();
            let progress = json!({"jsonrpc": "2.0", "method": "notifications/progress",
                                  "params": {"progressToken": "p", "progress": 1}});
            let done = json!({"jsonrpc": "2.0", "id": id,
                              "result": {"content": [{"type": "text", "text": "ok"}]}});
            Reply {
                status: "200 OK",
                content_type: "text/event-stream",
                headers: vec![],
                body: format!(": keep-alive\n\nevent: message\ndata: {progress}\n\ndata: {done}\n\n"),
            }
        }
        _ => status("404 Not Found", json!({"jsonrpc": "2.0", "id": req.body["id"],
                                             "error": {"code": -32601, "message": "Method not found"}})),
    }))
    .await;
    let client = connect(&server).await.expect("connects");
    assert_eq!(client.info().era, McpEra::Modern);
    assert_eq!(client.info().name.as_deref(), Some("remote"));

    let tools = client.list_tools().await.expect("tools");
    assert_eq!(
        tools.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
        vec!["query"],
        "tools with invalid x-mcp-header annotations are hidden"
    );
    let r = client
        .call_tool("query", json!({"region": "eu-north 1 ø"}), Some("p"))
        .await
        .expect("call");
    assert!(!r.is_error);

    let reqs = server.requests();
    let call = reqs
        .iter()
        .find(|r| r.rpc_method() == "tools/call")
        .expect("call");
    assert_eq!(call.header("mcp-protocol-version"), Some("2026-07-28"));
    assert_eq!(call.header("mcp-method"), Some("tools/call"));
    assert_eq!(call.header("mcp-name"), Some("query"));
    assert_eq!(
        call.header("mcp-param-region"),
        Some("=?base64?ZXUtbm9ydGggMSDDuA==?=")
    );
    assert_eq!(call.header("x-api-key"), Some("k"));
    assert!(call.header("mcp-session-id").is_none());
    assert_eq!(
        call.body["params"]["_meta"]["io.modelcontextprotocol/protocolVersion"],
        json!("2026-07-28")
    );
    assert!(
        call.header("accept")
            .is_some_and(|a| a.contains("application/json") && a.contains("text/event-stream"))
    );
}

#[tokio::test]
async fn legacy_http_uses_a_session() {
    let server = serve(Arc::new(|req: &Req| {
        if req.method == "GET" {
            return status("405 Method Not Allowed", json!({}));
        }
        if req.method == "DELETE" {
            return status("200 OK", json!({}));
        }
        match req.rpc_method() {
            "initialize" => {
                let mut r = result(
                    req,
                    json!({"protocolVersion": "2025-11-25", "capabilities": {"tools": {}},
                           "serverInfo": {"name": "old", "version": "1"}}),
                );
                r.headers.push(("Mcp-Session-Id", "sess-1".into()));
                r
            }
            "notifications/initialized" => Reply {
                status: "202 Accepted",
                content_type: "application/json",
                headers: vec![],
                body: String::new(),
            },
            _ if req.header("mcp-session-id") != Some("sess-1") => status(
                "400 Bad Request",
                json!({"jsonrpc": "2.0", "id": null,
                       "error": {"code": -32000, "message": "Bad Request: No valid session ID provided"}}),
            ),
            "tools/list" => result(req, json!({"tools": [{"name": "t", "inputSchema": {}}]})),
            _ => status("404 Not Found", json!({})),
        }
    }))
    .await;
    let client = connect(&server).await.expect("connects");
    assert_eq!(client.info().era, McpEra::Legacy);
    assert_eq!(client.info().version, "2025-11-25");
    assert_eq!(client.list_tools().await.expect("tools").len(), 1);
    client.close().await;

    let reqs = server.requests();
    let list = reqs
        .iter()
        .find(|r| r.rpc_method() == "tools/list")
        .expect("list");
    assert_eq!(list.header("mcp-protocol-version"), Some("2025-11-25"));
    assert!(list.body["params"].get("_meta").is_none());
    let init = reqs
        .iter()
        .find(|r| r.rpc_method() == "initialize")
        .expect("init");
    assert!(init.header("mcp-protocol-version").is_none());
    assert!(
        reqs.iter()
            .any(|r| r.method == "DELETE" && r.header("mcp-session-id") == Some("sess-1"))
    );
}

#[tokio::test]
async fn a_modern_server_listing_only_legacy_versions_gets_initialize() {
    let server = serve(Arc::new(|req: &Req| match req.rpc_method() {
        "server/discover" => status(
            "400 Bad Request",
            json!({"jsonrpc": "2.0", "id": req.body["id"], "error": {"code": -32022,
                   "message": "Unsupported protocol version",
                   "data": {"supported": ["2025-11-25"], "requested": "2026-07-28"}}}),
        ),
        "initialize" => result(
            req,
            json!({"protocolVersion": "2025-11-25", "capabilities": {}, "serverInfo": {"name": "d"}}),
        ),
        _ => Reply {
            status: "202 Accepted",
            content_type: "application/json",
            headers: vec![],
            body: String::new(),
        },
    }))
    .await;
    let client = connect(&server).await.expect("connects");
    assert_eq!(client.info().era, McpEra::Legacy);
}

#[tokio::test]
async fn unauthorized_servers_ask_for_a_sign_in() {
    let server = serve(Arc::new(|_req: &Req| Reply {
        status: "401 Unauthorized",
        content_type: "application/json",
        headers: vec![(
            "WWW-Authenticate",
            r#"Bearer resource_metadata="http://x/.well-known/oauth-protected-resource""#.into(),
        )],
        body: "{}".into(),
    }))
    .await;
    match connect(&server).await {
        Err(McpError::Unauthorized { challenge: Some(c) }) => {
            assert!(c.contains("resource_metadata"));
        }
        other => panic!("expected a sign-in error, got {:?}", other.err()),
    }
}

#[tokio::test]
async fn the_old_sse_transport_gets_a_clear_error() {
    let server = serve(Arc::new(|_req: &Req| {
        status("405 Method Not Allowed", json!({}))
    }))
    .await;
    let err = connect(&server).await.err().expect("fails");
    assert!(err.to_string().contains("HTTP+SSE"), "{err}");
}
