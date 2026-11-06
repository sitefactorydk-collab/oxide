//! End-to-end tests over stdio against `oxide-mcp-mock` (both protocol eras).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::future::BoxFuture;
use oxide_core::{McpContent, McpEra};
use oxide_mcp::jsonrpc::RpcError;
use oxide_mcp::log::ServerLog;
use oxide_mcp::process::ProcessSpec;
use oxide_mcp::transport::{StdioTransport, Transport};
use oxide_mcp::{ClientEvent, ClientHandler, ConnectOptions, McpClient, McpError};
use serde_json::{Map, Value, json};
use tokio::sync::mpsc;

#[derive(Default)]
struct TestUser {
    events: Mutex<Vec<ClientEvent>>,
}

impl ClientHandler for TestUser {
    fn sample<'a>(&'a self, _: &'a str, _: Value) -> BoxFuture<'a, Result<Value, RpcError>> {
        Box::pin(async {
            Ok(
                json!({"role": "assistant", "content": {"type": "text", "text": "sampled"},
                      "model": "test-model", "stopReason": "endTurn"}),
            )
        })
    }
    fn elicit<'a>(&'a self, _: &'a str, _: Value) -> BoxFuture<'a, Result<Value, RpcError>> {
        Box::pin(async { Ok(json!({"action": "accept", "content": {"color": "blue"}})) })
    }
    fn roots<'a>(&'a self, _: &'a str) -> BoxFuture<'a, Vec<Value>> {
        Box::pin(async { vec![json!({"uri": "file:///tmp", "name": "tmp"})] })
    }
    fn event(&self, _: &str, event: ClientEvent) {
        self.events.lock().expect("lock").push(event);
    }
}

impl TestUser {
    fn events(&self) -> Vec<ClientEvent> {
        self.events.lock().expect("lock").clone()
    }
}

async fn connect(
    mode: &str,
    opts: ConnectOptions,
) -> (
    Result<Arc<McpClient>, McpError>,
    Arc<TestUser>,
    Arc<ServerLog>,
) {
    let (tx, rx) = mpsc::unbounded_channel();
    let log = ServerLog::memory();
    let spec = ProcessSpec {
        command: env!("CARGO_BIN_EXE_oxide-mcp-mock").into(),
        args: vec![mode.into()],
        env: Default::default(),
        cwd: None,
    };
    let transport = StdioTransport::spawn(&spec, tx, log.clone()).expect("spawns");
    let user = Arc::new(TestUser::default());
    let client =
        McpClient::connect("mock", Transport::Stdio(transport), rx, user.clone(), opts).await;
    (client, user, log)
}

fn text(c: &[McpContent]) -> String {
    match c.first() {
        Some(McpContent::Text { text }) => text.clone(),
        other => panic!("not text: {other:?}"),
    }
}

async fn eventually(mut check: impl FnMut() -> bool) -> bool {
    for _ in 0..100 {
        if check() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    false
}

#[tokio::test]
async fn modern_server_end_to_end() {
    let (client, user, _log) = connect("modern", ConnectOptions::default()).await;
    let client = client.expect("connects");
    let info = client.info();
    assert_eq!(info.era, McpEra::Modern);
    assert_eq!(info.version, "2026-07-28");
    assert_eq!(info.name.as_deref(), Some("mock"));
    assert_eq!(info.instructions.as_deref(), Some("A test server."));

    let tools = client.list_tools().await.expect("tools");
    assert_eq!(tools.len(), 6, "both pages are read");
    assert_eq!(tools[1].annotations.read_only_hint, Some(true));

    let r = client
        .call_tool("echo", json!({"text": "hi"}), None)
        .await
        .expect("echo");
    assert_eq!(text(&r.content), "hi");
    let r = client
        .call_tool("add", json!({"a": 2, "b": 3}), None)
        .await
        .expect("add");
    assert_eq!(r.structured.as_deref(), Some(r#"{"sum":5.0}"#));

    // Multi round-trip: elicitation and sampling are gathered and the call is retried.
    let r = client.call_tool("ask", json!({}), None).await.expect("ask");
    assert_eq!(text(&r.content), r#"accept blue state="state-1""#);
    let r = client
        .call_tool("sample", json!({}), None)
        .await
        .expect("sample");
    assert_eq!(text(&r.content), "sampled");

    let read = client.read_resource("mock://readme").await.expect("read");
    assert!(
        matches!(&read.contents[0], McpContent::Resource { text: Some(t), .. } if t == "Read me.")
    );
    let mut args = Map::new();
    args.insert("name".into(), json!("Ada"));
    let p = client.get_prompt("greet", &args).await.expect("prompt");
    assert_eq!(text(&[p.messages[0].content.clone()]), "Say hello to Ada.");

    let r = client
        .call_tool("slow", json!({"steps": 3}), Some("call-1"))
        .await
        .expect("slow");
    assert_eq!(text(&r.content), "done");
    assert!(eventually(|| user.events().iter().any(|e| matches!(e,
        ClientEvent::Progress { token, progress, total: Some(t), .. } if token == "call-1" && *progress == 3.0 && *t == 3.0)))
    .await);
    // The subscription announced a tools change.
    assert!(eventually(|| user.events().contains(&ClientEvent::ListChanged)).await);

    client.ping().await.expect("ping");
    let mut closed = client.closed();
    client.close().await;
    assert!(closed.wait_for(Option::is_some).await.is_ok());
}

#[tokio::test]
async fn legacy_server_end_to_end() {
    let (client, user, _log) = connect("legacy", ConnectOptions::default()).await;
    let client = client.expect("connects");
    assert_eq!(client.info().era, McpEra::Legacy);
    assert_eq!(client.info().version, "2025-06-18");
    assert_eq!(client.info().name.as_deref(), Some("mock-legacy"));
    assert_eq!(client.list_tools().await.expect("tools").len(), 6);

    // Server-to-client requests on the same channel.
    let r = client.call_tool("ask", json!({}), None).await.expect("ask");
    assert_eq!(text(&r.content), "accept blue state=null");
    let r = client
        .call_tool("sample", json!({}), None)
        .await
        .expect("sample");
    assert_eq!(text(&r.content), "sampled");
    let r = client
        .call_tool("roots", json!({}), None)
        .await
        .expect("roots");
    assert!(text(&r.content).contains("file:///tmp"));
    assert!(
        eventually(|| user.events().iter().any(|e| matches!(e,
        ClientEvent::Log { text, .. } if text == "asking for a colour")))
        .await
    );
    client.close().await;
}

#[tokio::test]
async fn silent_legacy_server_falls_back_after_the_probe_times_out() {
    let opts = ConnectOptions {
        probe_timeout: Duration::from_millis(500),
        ..ConnectOptions::default()
    };
    let (client, _user, _log) = connect("legacy-silent", opts).await;
    assert_eq!(client.expect("connects").info().era, McpEra::Legacy);
}

#[tokio::test]
async fn a_legacy_server_that_dies_on_the_probe_can_be_retried_as_legacy() {
    let (client, _user, _log) = connect("legacy-crash", ConnectOptions::default()).await;
    assert!(
        matches!(client, Err(McpError::Closed(_))),
        "{:?}",
        client.err()
    );
    let opts = ConnectOptions {
        prefer: Some(McpEra::Legacy),
        ..ConnectOptions::default()
    };
    let (client, _user, _log) = connect("legacy-crash", opts).await;
    assert_eq!(client.expect("connects").info().era, McpEra::Legacy);
}

#[tokio::test]
async fn dropping_a_call_cancels_it_on_the_server() {
    for mode in ["modern", "legacy"] {
        let (client, _user, log) = connect(mode, ConnectOptions::default()).await;
        let client = client.expect("connects");
        let call = client.call_tool("slow", json!({"steps": 50}), None);
        assert!(
            tokio::time::timeout(Duration::from_millis(250), call)
                .await
                .is_err()
        );
        assert!(
            eventually(|| log.lines().iter().any(|l| l.text.starts_with("cancelled"))).await,
            "{mode}: {:?}",
            log.lines()
        );
        client.close().await;
    }
}

#[tokio::test]
async fn missing_programs_get_a_helpful_error() {
    let (tx, _rx) = mpsc::unbounded_channel();
    let spec = ProcessSpec {
        command: "definitely-not-a-real-program-xyz".into(),
        args: vec![],
        env: Default::default(),
        cwd: None,
    };
    let err = StdioTransport::spawn(&spec, tx, ServerLog::memory())
        .err()
        .expect("fails");
    assert!(err.to_string().contains("Couldn't find"), "{err}");
}
