//! A WebSocket client that stands in for the Chrome extension (tests here and in
//! `oxide-agent`, behind the `testing` feature).

use std::sync::Arc;

use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};

use crate::ws::{self, Message, Reader};

/// An origin that passes the bridge's check.
pub const TEST_ORIGIN: &str = "chrome-extension://abcdefghijklmnopabcdefghijklmnop";

pub struct TestClient {
    reader: Reader<OwnedReadHalf>,
    writer: OwnedWriteHalf,
    next_id: u64,
}

impl TestClient {
    /// Opens a WebSocket to the bridge. `Err` holds the HTTP status line of a refusal.
    pub async fn connect(port: u16, origin: Option<&str>) -> Result<Self, String> {
        Self::connect_with(port, origin, &format!("127.0.0.1:{port}")).await
    }

    pub async fn connect_with(port: u16, origin: Option<&str>, host: &str) -> Result<Self, String> {
        let stream = TcpStream::connect(("127.0.0.1", port))
            .await
            .map_err(|e| e.to_string())?;
        let (mut rd, mut wr) = stream.into_split();
        let mut req = format!(
            "GET / HTTP/1.1\r\nHost: {host}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
             Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n"
        );
        if let Some(o) = origin {
            req.push_str(&format!("Origin: {o}\r\n"));
        }
        req.push_str("\r\n");
        wr.write_all(req.as_bytes())
            .await
            .map_err(|e| e.to_string())?;
        let mut head = Vec::new();
        let mut b = [0u8; 1];
        while !head.ends_with(b"\r\n\r\n") {
            if rd.read(&mut b).await.map_err(|e| e.to_string())? == 0 {
                break;
            }
            head.push(b[0]);
        }
        let text = String::from_utf8_lossy(&head).into_owned();
        let status = text.lines().next().unwrap_or("").to_owned();
        if !status.contains(" 101 ") {
            return Err(status);
        }
        if !text.contains(&ws::accept_key("dGhlIHNhbXBsZSBub25jZQ==")) {
            return Err("bad accept key".into());
        }
        Ok(Self {
            reader: Reader::new(rd, false, 64 * 1024 * 1024),
            writer: wr,
            next_id: 1,
        })
    }

    pub async fn send(&mut self, v: &Value) {
        self.send_raw(&v.to_string()).await;
    }

    pub async fn send_raw(&mut self, text: &str) {
        let _ = ws::write_frame(
            &mut self.writer,
            ws::OP_TEXT,
            text.as_bytes(),
            Some([7, 1, 9, 3]),
        )
        .await;
    }

    /// The next message: JSON text (pings answered), or `Err` with the close code (0 when
    /// the connection just ended).
    pub async fn recv(&mut self) -> Result<Value, u16> {
        loop {
            match self.reader.next().await {
                Ok(Message::Text(t)) => return serde_json::from_str(&t).map_err(|_| 0),
                Ok(Message::Ping(p)) => {
                    let _ = ws::write_frame(&mut self.writer, ws::OP_PONG, &p, Some([1, 2, 3, 4]))
                        .await;
                }
                Ok(Message::Close(c)) => return Err(c.map_or(0, |(code, _)| code)),
                Ok(_) => {}
                Err(_) => return Err(0),
            }
        }
    }

    /// Sends a request and waits for its answer (other messages are dropped).
    pub async fn request(&mut self, method: &str, params: Value) -> Result<Value, u16> {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))
            .await;
        loop {
            let v = self.recv().await?;
            if v.get("id") == Some(&json!(id)) {
                return Ok(v);
            }
        }
    }

    /// Pairs with `code`: `Ok((browser id, token))` or the error message.
    pub async fn pair(&mut self, code: &str) -> Result<(String, String), String> {
        let v = self
            .request(
                "pair",
                json!({"code": code, "name": "Test Chrome", "version": "0.1.0",
                       "protocol": crate::PROTOCOL}),
            )
            .await
            .map_err(|c| format!("closed {c}"))?;
        match v.get("result") {
            Some(r) => Ok((
                r["browserId"].as_str().unwrap_or("").to_owned(),
                r["token"].as_str().unwrap_or("").to_owned(),
            )),
            None => Err(v["error"]["message"].as_str().unwrap_or("").to_owned()),
        }
    }

    pub async fn hello(&mut self, browser_id: &str, token: &str) -> Result<Value, String> {
        let v = self
            .request(
                "hello",
                json!({"browserId": browser_id, "token": token, "name": "Test Chrome",
                       "version": "0.1.0", "protocol": crate::PROTOCOL}),
            )
            .await
            .map_err(|c| format!("closed {c}"))?;
        match v.get("result") {
            Some(r) => Ok(r.clone()),
            None => Err(v["error"]["message"].as_str().unwrap_or("").to_owned()),
        }
    }

    /// Answers `tools/call` requests with `handler(session, tool, arguments)` until the
    /// connection ends.
    pub async fn serve_tools<F>(mut self, handler: F)
    where
        F: Fn(&str, &str, &Value) -> Value + Send + Sync,
    {
        while let Ok(v) = self.recv().await {
            if v.get("method") == Some(&json!("tools/call")) {
                let p = &v["params"];
                let result = handler(
                    p["session"].as_str().unwrap_or(""),
                    p["name"].as_str().unwrap_or(""),
                    &p["arguments"],
                );
                self.send(&json!({"jsonrpc": "2.0", "id": v["id"], "result": result}))
                    .await;
            }
        }
    }
}

/// Pairs a stand-in extension with `bridge` and answers tool calls with `handler` in the
/// background. Returns once it is connected.
pub async fn fake_extension<F>(bridge: &crate::Bridge, handler: F) -> tokio::task::JoinHandle<()>
where
    F: Fn(&str, &str, &Value) -> Value + Send + Sync + 'static,
{
    let port = bridge.port().expect("the bridge is listening");
    let code = bridge.start_pairing().code;
    let mut c = TestClient::connect(port, Some(TEST_ORIGIN))
        .await
        .expect("connects");
    c.pair(&code).await.expect("pairs");
    let handler = Arc::new(handler);
    let task = tokio::spawn(async move { c.serve_tools(move |s, t, a| handler(s, t, a)).await });
    bridge
        .wait_connected(std::time::Duration::from_secs(5))
        .await;
    task
}

/// A text result as the extension sends it.
pub fn text_result(text: &str) -> Value {
    json!({"content": [{"type": "text", "text": text}], "isError": false})
}
