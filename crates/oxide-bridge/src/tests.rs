//! The bridge against a stand-in extension over real sockets.

use std::sync::Mutex as StdMutex;

use oxide_store::MemorySecrets;
use serde_json::json;

use super::*;
use crate::testing::{TEST_ORIGIN, TestClient, text_result};

#[derive(Default)]
struct Recorder {
    stopped: StdMutex<Vec<String>>,
    changes: StdMutex<usize>,
}

impl BridgeSink for Recorder {
    fn changed(&self, _: &ChromeStatus) {
        *lock(&self.changes) += 1;
    }
    fn stopped(&self, session: &str) {
        lock(&self.stopped).push(session.to_owned());
    }
}

struct Fixture {
    bridge: Bridge,
    secrets: Arc<MemorySecrets>,
    sink: Arc<Recorder>,
    port: u16,
    _dir: tempfile::TempDir,
}

async fn free_port() -> u16 {
    let l = TcpListener::bind(("127.0.0.1", 0)).await.expect("binds");
    l.local_addr().expect("addr").port()
}

async fn fixture() -> Fixture {
    let dir = tempfile::tempdir().expect("tempdir");
    let secrets = Arc::new(MemorySecrets::default());
    let sink = Arc::new(Recorder::default());
    let bridge = Bridge::open(
        dir.path().join("chrome.json"),
        secrets.clone(),
        sink.clone(),
    )
    .await;
    let port = free_port().await;
    bridge
        .configure(&ChromeSettings {
            enabled: true,
            port,
            ask: true,
        })
        .await;
    assert_eq!(bridge.port(), Some(port), "{:?}", bridge.status().error);
    Fixture {
        bridge,
        secrets,
        sink,
        port,
        _dir: dir,
    }
}

/// Pairs a client: returns it (connected) and its browser id and token.
async fn paired(f: &Fixture) -> (TestClient, String, String) {
    let code = f.bridge.start_pairing().code;
    let mut c = TestClient::connect(f.port, Some(TEST_ORIGIN))
        .await
        .expect("connects");
    let (id, token) = c.pair(&code).await.expect("pairs");
    (c, id, token)
}

#[tokio::test]
async fn foreign_origins_hosts_and_plain_http_are_refused() {
    let f = fixture().await;
    for origin in [
        None,
        Some("https://evil.example"),
        Some("http://127.0.0.1"),
        Some("chrome-extension://short"),
        Some("chrome-extension://ABCDEFGHIJKLMNOPABCDEFGHIJKLMNOP"),
        Some("moz-extension://abcdefghijklmnopabcdefghijklmnop"),
    ] {
        let r = TestClient::connect(f.port, origin).await;
        assert!(
            matches!(&r, Err(s) if s.contains("403")),
            "{origin:?}: {:?}",
            r.as_ref().err()
        );
    }
    // A page on another host name (DNS rebinding) is refused even with a good origin.
    let r = TestClient::connect_with(f.port, Some(TEST_ORIGIN), "evil.example:80").await;
    assert!(matches!(&r, Err(s) if s.contains("403")));
    let r = TestClient::connect_with(f.port, Some(TEST_ORIGIN), "127.0.0.1:1").await;
    assert!(matches!(&r, Err(s) if s.contains("403")));
    // Plain HTTP.
    let mut s = TcpStream::connect(("127.0.0.1", f.port))
        .await
        .expect("tcp");
    s.write_all(b"GET / HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
        .await
        .expect("writes");
    let mut out = String::new();
    tokio::io::AsyncReadExt::read_to_string(&mut s, &mut out)
        .await
        .expect("reads");
    assert!(out.starts_with("HTTP/1.1 400"), "{out}");
    // `localhost` with the right port and a good origin is fine.
    assert!(
        TestClient::connect_with(f.port, Some(TEST_ORIGIN), &format!("localhost:{}", f.port))
            .await
            .is_ok()
    );
    assert!(!f.bridge.connected());
}

#[tokio::test]
async fn unpaired_connections_are_refused() {
    let f = fixture().await;
    // A made-up token.
    let mut c = TestClient::connect(f.port, Some(TEST_ORIGIN))
        .await
        .expect("connects");
    let e = c
        .hello("someone", "not-a-token")
        .await
        .expect_err("refused");
    assert!(e.contains("doesn't know this pairing"), "{e}");
    assert_eq!(c.recv().await, Err(close::UNAUTHORIZED));
    // A tool answer or call before pairing.
    let mut c = TestClient::connect(f.port, Some(TEST_ORIGIN))
        .await
        .expect("connects");
    let v = c
        .request("tools/call", json!({"name": "read_page"}))
        .await
        .expect("answer");
    assert_eq!(v["error"]["code"], json!(codes::UNAUTHORIZED));
    assert_eq!(c.recv().await, Err(close::UNAUTHORIZED));
    // Garbage.
    let mut c = TestClient::connect(f.port, Some(TEST_ORIGIN))
        .await
        .expect("connects");
    c.send_raw("not json").await;
    let v = c.recv().await.expect("answer");
    assert_eq!(v["error"]["code"], json!(codes::UNAUTHORIZED));
    // Silence: closed after the time limit.
    let mut c = TestClient::connect(f.port, Some(TEST_ORIGIN))
        .await
        .expect("connects");
    let started = Instant::now();
    assert_eq!(c.recv().await, Err(close::POLICY));
    assert!(started.elapsed() >= AUTH_TIMEOUT);
    // Pairing without a code waiting.
    let mut c = TestClient::connect(f.port, Some(TEST_ORIGIN))
        .await
        .expect("connects");
    let e = c
        .pair("amber river toast mint lake ocean")
        .await
        .expect_err("refused");
    assert!(e.contains("isn't waiting"), "{e}");
    // An old protocol.
    let code = f.bridge.start_pairing().code;
    let mut c = TestClient::connect(f.port, Some(TEST_ORIGIN))
        .await
        .expect("connects");
    let v = c
        .request("pair", json!({"code": code, "protocol": 0}))
        .await
        .expect("answer");
    assert_eq!(v["error"]["code"], json!(codes::PROTOCOL));
    assert!(!f.bridge.connected());
    assert!(f.bridge.status().paired.is_empty());
    let e = f
        .bridge
        .call("chat", "read_page", json!({}))
        .await
        .expect_err("not connected");
    assert!(matches!(e, BridgeError::NotConnected), "{e}");
}

#[tokio::test]
async fn pairing_gives_a_token_bound_to_the_extension() {
    let f = fixture().await;
    let code = f.bridge.start_pairing().code;
    assert_eq!(
        f.bridge.status().pairing.map(|p| p.code),
        Some(code.clone())
    );
    // Typed with capitals and odd spacing still works.
    let mut c = TestClient::connect(f.port, Some(TEST_ORIGIN))
        .await
        .expect("connects");
    let (id, token) = c
        .pair(&format!("  {}  ", code.to_uppercase().replace(' ', "-")))
        .await
        .expect("pairs");
    assert_eq!(token.len(), 43, "256 bits base64url");
    assert!(f.bridge.connected());
    let st = f.bridge.status();
    assert_eq!(st.connected.as_deref(), Some("Test Chrome"));
    assert_eq!(st.extension_version.as_deref(), Some("0.1.0"));
    assert!(st.pairing.is_none(), "a code is used once");
    assert_eq!(st.paired.len(), 1);
    assert_eq!(
        st.paired[0].extension_id,
        "abcdefghijklmnopabcdefghijklmnop"
    );
    assert_eq!(
        f.secrets
            .get(&format!("chrome-token-{id}"))
            .expect("keychain")
            .map(|s| s.expose().to_owned()),
        Some(token.clone())
    );
    // The code can't be used again.
    let mut again = TestClient::connect(f.port, Some(TEST_ORIGIN))
        .await
        .expect("connects");
    assert!(again.pair(&code).await.is_err());

    // Reconnecting with the token replaces the first connection.
    let mut c2 = TestClient::connect(f.port, Some(TEST_ORIGIN))
        .await
        .expect("connects");
    c2.hello(&id, &token).await.expect("hello");
    assert_eq!(c.recv().await, Err(close::REPLACED));
    assert!(f.bridge.connected());

    // The same token from another extension, or a wrong one, is refused.
    let other = "chrome-extension://ponmlkjihgfedcbaponmlkjihgfedcba";
    let mut c3 = TestClient::connect(f.port, Some(other))
        .await
        .expect("connects");
    assert!(c3.hello(&id, &token).await.is_err());
    let mut c4 = TestClient::connect(f.port, Some(TEST_ORIGIN))
        .await
        .expect("connects");
    let mut bad = token.clone();
    bad.replace_range(0..1, if bad.starts_with('A') { "B" } else { "A" });
    assert!(c4.hello(&id, &bad).await.is_err());

    // The pairing survives a restart (list in the file, token in the keychain).
    let file = f._dir.path().join("chrome.json");
    let reopened = Bridge::open(file, f.secrets.clone(), Arc::new(NoSink)).await;
    assert_eq!(reopened.status().paired.len(), 1);

    // Forgetting it closes the connection and the token stops working.
    f.bridge.forget(&id).await;
    assert_eq!(c2.recv().await, Err(close::FORGOTTEN));
    assert!(f.bridge.status().paired.is_empty());
    assert!(
        f.secrets
            .get(&format!("chrome-token-{id}"))
            .expect("keychain")
            .is_none()
    );
    let mut c5 = TestClient::connect(f.port, Some(TEST_ORIGIN))
        .await
        .expect("connects");
    assert!(c5.hello(&id, &token).await.is_err());
}

#[tokio::test]
async fn wrong_and_expired_codes_are_refused() {
    let f = fixture().await;
    let code = f.bridge.start_pairing().code;
    let wrong = if code.starts_with("acorn") {
        "amber amber amber amber amber amber"
    } else {
        "acorn acorn acorn acorn acorn acorn"
    };
    for i in 0..PAIRING_TRIES {
        let mut c = TestClient::connect(f.port, Some(TEST_ORIGIN))
            .await
            .expect("connects");
        let e = c.pair(wrong).await.expect_err("wrong");
        if i + 1 < PAIRING_TRIES {
            assert!(e.contains("isn't the code"), "{e}");
        } else {
            assert!(e.contains("Too many"), "{e}");
        }
        assert_eq!(c.recv().await, Err(close::UNAUTHORIZED));
    }
    // Withdrawn: even the right code no longer works.
    let mut c = TestClient::connect(f.port, Some(TEST_ORIGIN))
        .await
        .expect("connects");
    assert!(
        c.pair(&code)
            .await
            .expect_err("withdrawn")
            .contains("isn't waiting")
    );
    assert!(f.bridge.status().pairing.is_none());

    // An expired code.
    let code = f.bridge.start_pairing().code;
    if let Some(p) = lock(&f.bridge.inner.state).pairing.as_mut() {
        p.expires_at = Instant::now() - Duration::from_secs(1);
    }
    assert!(f.bridge.status().pairing.is_none());
    let mut c = TestClient::connect(f.port, Some(TEST_ORIGIN))
        .await
        .expect("connects");
    assert!(c.pair(&code).await.is_err());
    // A cancelled one.
    let code = f.bridge.start_pairing().code;
    f.bridge.cancel_pairing();
    let mut c = TestClient::connect(f.port, Some(TEST_ORIGIN))
        .await
        .expect("connects");
    assert!(c.pair(&code).await.is_err());
    assert!(!f.bridge.connected());
}

#[tokio::test]
async fn tools_are_called_and_answered() {
    let f = fixture().await;
    let (c, _, _) = paired(&f).await;
    let ext = tokio::spawn(c.serve_tools(|session, tool, args| match tool {
        "screenshot" => json!({"content": [
            {"type": "text", "text": format!("{session} shot")},
            {"type": "image", "data": "aGk=", "mimeType": "image/jpeg"},
            {"type": "somethingNew", "x": 1}
        ]}),
        "click" => json!({"content": [{"type": "text", "text": "No element 9."}], "isError": true}),
        _ => text_result(&format!("{tool} {args}")),
    }));
    let r = f
        .bridge
        .call("chat-1", "read_page", json!({"start": 5}))
        .await
        .expect("answer");
    assert_eq!(
        r.content,
        vec![McpContent::Text {
            text: "read_page {\"start\":5}".into()
        }]
    );
    assert!(!r.is_error);
    assert_eq!(f.bridge.status().sessions, vec!["chat-1".to_owned()]);
    let r = f
        .bridge
        .call("chat-1", "screenshot", json!({}))
        .await
        .expect("answer");
    assert_eq!(r.content.len(), 2, "unknown content is skipped");
    assert!(
        matches!(&r.content[1], McpContent::Image { mime_type, .. } if mime_type == "image/jpeg")
    );
    let r = f
        .bridge
        .call("chat-2", "click", json!({"ref": 9}))
        .await
        .expect("answer");
    assert!(r.is_error);
    // Calls from several chats at once.
    let (a, b) = tokio::join!(
        f.bridge.call("chat-1", "find", json!({"query": "a"})),
        f.bridge.call("chat-2", "find", json!({"query": "b"})),
    );
    assert!(a.is_ok() && b.is_ok());
    f.bridge.release("chat-1").await;
    assert_eq!(f.bridge.status().sessions, vec!["chat-2".to_owned()]);
    ext.abort();
}

#[tokio::test]
async fn release_stop_and_errors_reach_both_sides() {
    let f = fixture().await;
    let (mut c, _, _) = paired(&f).await;
    // A call the extension answers with a JSON-RPC error.
    let bridge = f.bridge.clone();
    let call = tokio::spawn(async move { bridge.call("chat-9", "navigate", json!({})).await });
    let req = c.recv().await.expect("call");
    assert_eq!(req["method"], json!("tools/call"));
    assert_eq!(req["params"]["session"], json!("chat-9"));
    c.send(&json!({"jsonrpc": "2.0", "id": req["id"],
                   "error": {"code": -32000, "message": "Stopped by the user"}}))
        .await;
    let e = call.await.expect("joins").expect_err("error");
    assert!(e.to_string().contains("Stopped by the user"), "{e}");
    // The extension's heartbeat.
    let v = c.request("ping", json!({})).await.expect("pong");
    assert_eq!(v["result"], json!("pong"));
    // Unknown methods.
    let v = c.request("nonsense", json!({})).await.expect("answer");
    assert_eq!(v["error"]["code"], json!(codes::NO_METHOD));
    // Stop pressed in the tab.
    c.send(&json!({"jsonrpc": "2.0", "method": "session/stopped",
                   "params": {"session": "chat-9"}}))
        .await;
    for _ in 0..50 {
        if !lock(&f.sink.stopped).is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(*lock(&f.sink.stopped), vec!["chat-9".to_owned()]);
    assert!(f.bridge.status().sessions.is_empty());
    // Releasing a stopped chat reaches the extension (so a new reply may use Chrome).
    f.bridge.release("chat-9").await;
    let v = c.recv().await.expect("release");
    assert_eq!(v["method"], json!("session/release"));
    assert_eq!(v["params"]["session"], json!("chat-9"));
    // Release reaches the extension.
    let bridge = f.bridge.clone();
    let call = tokio::spawn(async move { bridge.call("chat-3", "tabs_list", json!({})).await });
    let req = c.recv().await.expect("call");
    c.send(&json!({"jsonrpc": "2.0", "id": req["id"], "result": text_result("tabs")}))
        .await;
    call.await.expect("joins").expect("ok");
    f.bridge.release("chat-3").await;
    let v = c.recv().await.expect("release");
    assert_eq!(v["method"], json!("session/release"));
    assert_eq!(v["params"]["session"], json!("chat-3"));
    // Disconnecting during a call fails it.
    let bridge = f.bridge.clone();
    let call = tokio::spawn(async move { bridge.call("chat-3", "click", json!({})).await });
    let _ = c.recv().await.expect("call");
    drop(c);
    let e = call.await.expect("joins").expect_err("disconnected");
    assert!(matches!(e, BridgeError::Disconnected), "{e}");
    assert!(f.bridge.wait_disconnected().await);
    assert!(*lock(&f.sink.changes) > 3);
}

#[tokio::test]
async fn a_silent_extension_is_dropped() {
    let f = fixture().await;
    let (c, _, _) = paired(&f).await;
    assert!(f.bridge.connected());
    // Never reads (so never answers pings) and never sends.
    let started = Instant::now();
    assert!(f.bridge.wait_disconnected().await);
    assert!(started.elapsed() >= IDLE_TIMEOUT);
    drop(c);
}

#[tokio::test]
async fn a_live_extension_stays_connected() {
    let f = fixture().await;
    let (mut c, _, _) = paired(&f).await;
    // Answering pings (recv does) keeps it alive past the idle limit.
    let until = Instant::now() + IDLE_TIMEOUT * 3;
    while Instant::now() < until {
        let _ = tokio::time::timeout(Duration::from_millis(50), c.recv()).await;
    }
    assert!(f.bridge.connected());
}

#[tokio::test]
async fn settings_start_move_and_stop_the_listener() {
    let f = fixture().await;
    let (mut c, _, _) = paired(&f).await;
    // Another port: the old connection closes, the new port listens.
    let port2 = free_port().await;
    f.bridge
        .configure(&ChromeSettings {
            enabled: true,
            port: port2,
            ask: true,
        })
        .await;
    assert_eq!(f.bridge.port(), Some(port2));
    assert_eq!(c.recv().await, Err(close::GOING_AWAY));
    assert!(TestClient::connect(port2, Some(TEST_ORIGIN)).await.is_ok());
    // Off.
    f.bridge
        .configure(&ChromeSettings {
            enabled: false,
            port: port2,
            ask: true,
        })
        .await;
    assert_eq!(f.bridge.port(), None);
    assert!(!f.bridge.status().listening);
    // A port that is taken.
    let taken = TcpListener::bind(("127.0.0.1", 0)).await.expect("binds");
    let port3 = taken.local_addr().expect("addr").port();
    f.bridge
        .configure(&ChromeSettings {
            enabled: true,
            port: port3,
            ask: true,
        })
        .await;
    let st = f.bridge.status();
    assert!(!st.listening);
    assert!(
        st.error.as_deref().is_some_and(|e| e.contains("in use")),
        "{:?}",
        st.error
    );
}

#[test]
fn origins_and_hosts() {
    assert_eq!(
        extension_id("chrome-extension://abcdefghijklmnopabcdefghijklmnop/"),
        Some("abcdefghijklmnopabcdefghijklmnop")
    );
    assert_eq!(
        extension_id("chrome-extension://abcdefghijklmnopabcdefghijklmnoq"),
        None
    );
    assert!(local_host("LOCALHOST:5", 5) && local_host("[::1]:5", 5));
    assert!(!local_host("127.0.0.1", 5) && !local_host("127.0.0.1.nip.io:5", 5));
    assert!(same_secret("abc", "abc") && !same_secret("abc", "abd") && !same_secret("a", "ab"));
}

/// Random handshakes, random first messages and random frames from many clients: every
/// one is refused or closed, nothing panics, and pairing still works afterwards (Phase 18
/// fuzzing).
#[tokio::test]
async fn garbage_from_many_clients_never_breaks_the_bridge() {
    use rand::{Rng, SeedableRng};
    use tokio::io::AsyncReadExt;
    let f = fixture().await;
    let mut rng = rand::rngs::StdRng::seed_from_u64(0xb71d6e);
    let words = [
        "GET", "/", "HTTP/1.1", "\r\n", "Host: 127.0.0.1", "Upgrade: websocket",
        "Connection: Upgrade", "Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==",
        "Sec-WebSocket-Version: 13", "Origin: ", TEST_ORIGIN, ":", "\0", "é", "\r\n\r\n",
    ];
    // 1. Random request heads over raw TCP.
    for _ in 0..60 {
        let mut head = String::new();
        for _ in 0..rng.gen_range(1..25) {
            head.push_str(words[rng.gen_range(0..words.len())]);
            if rng.gen_bool(0.3) {
                head.push(' ');
            }
        }
        if rng.gen_bool(0.5) {
            head.push_str("\r\n\r\n");
        }
        let Ok(mut s) = TcpStream::connect(("127.0.0.1", f.port)).await else {
            continue;
        };
        let _ = s.write_all(head.as_bytes()).await;
        let mut buf = vec![0u8; 512];
        let _ = tokio::time::timeout(Duration::from_millis(300), s.read(&mut buf)).await;
    }
    // 2. A good handshake, then random text as the first message.
    for _ in 0..40 {
        let mut c = TestClient::connect(f.port, Some(TEST_ORIGIN))
            .await
            .expect("connects");
        let junk: String = match rng.gen_range(0..4) {
            0 => (0..rng.gen_range(0..200)).map(|_| rng.gen_range(' '..='~')).collect(),
            1 => json!({"jsonrpc": "2.0", "id": rng.gen_range(0..9), "method": "hello",
                        "params": {"browserId": "x".repeat(rng.gen_range(0..300)), "token": rng.r#gen::<u64>().to_string(), "protocol": 1}})
                .to_string(),
            2 => json!({"jsonrpc": "2.0", "id": 1, "method": "pair",
                        "params": {"code": "amber amber amber amber amber amber", "protocol": rng.gen_range(-2..4)}})
                .to_string(),
            _ => {
                let method = ["tools/call", "session/stopped", "ping", "x"][rng.gen_range(0..4)];
                let params = [json!(1), json!({"a": []}), json!(null)][rng.gen_range(0..3)].clone();
                json!({"jsonrpc": "2.0", "id": null, "method": method, "params": params}).to_string()
            }
        };
        c.send_raw(&junk).await;
        // Answered with an error or closed; never left hanging past the auth limit.
        let got = tokio::time::timeout(AUTH_TIMEOUT + Duration::from_secs(2), c.recv()).await;
        assert!(got.is_ok(), "no answer to {junk}");
    }
    // 3. A good handshake, then random binary frames.
    for _ in 0..30 {
        let Ok(mut s) = TcpStream::connect(("127.0.0.1", f.port)).await else {
            continue;
        };
        let head = format!(
            "GET / HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
             Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\nOrigin: {TEST_ORIGIN}\r\n\r\n",
            f.port
        );
        let _ = s.write_all(head.as_bytes()).await;
        let noise: Vec<u8> = (0..rng.gen_range(1..2_000)).map(|_| rng.r#gen()).collect();
        let _ = s.write_all(&noise).await;
        let mut buf = vec![0u8; 4096];
        let _ = tokio::time::timeout(Duration::from_millis(300), s.read(&mut buf)).await;
    }
    assert!(!f.bridge.connected());
    assert!(f.bridge.status().paired.is_empty());
    // Still pairs.
    let (_c, id, _token) = paired(&f).await;
    assert!(f.bridge.connected());
    assert_eq!(f.bridge.status().paired.len(), 1, "{id}");
}
