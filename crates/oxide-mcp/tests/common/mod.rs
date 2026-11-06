//! A scripted HTTP server for the integration tests.
#![allow(dead_code)]

use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[derive(Debug, Clone)]
pub struct Req {
    pub method: String,
    pub path: String,
    pub text: String,
    pub headers: Vec<(String, String)>,
    pub body: Value,
}

impl Req {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
    pub fn rpc_method(&self) -> &str {
        self.body
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or("")
    }
}

pub struct Reply {
    pub status: &'static str,
    pub content_type: &'static str,
    pub headers: Vec<(&'static str, String)>,
    pub body: String,
}

pub fn json_reply(body: Value) -> Reply {
    Reply {
        status: "200 OK",
        content_type: "application/json",
        headers: vec![],
        body: body.to_string(),
    }
}

pub fn result(req: &Req, result: Value) -> Reply {
    json_reply(json!({"jsonrpc": "2.0", "id": req.body["id"], "result": result}))
}

pub fn status(status: &'static str, body: Value) -> Reply {
    Reply {
        status,
        content_type: "application/json",
        headers: vec![],
        body: body.to_string(),
    }
}

pub type Handler = Arc<dyn Fn(&Req) -> Reply + Send + Sync>;

pub struct TestServer {
    pub url: String,
    pub origin: String,
    log: Arc<Mutex<Vec<Req>>>,
}

impl TestServer {
    pub fn requests(&self) -> Vec<Req> {
        self.log.lock().expect("lock").clone()
    }
}

pub async fn serve(handler: Handler) -> TestServer {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("binds");
    let addr = listener.local_addr().expect("addr");
    let log = Arc::new(Mutex::new(Vec::new()));
    let l2 = log.clone();
    tokio::spawn(async move {
        while let Ok((mut sock, _)) = listener.accept().await {
            let handler = handler.clone();
            let log = l2.clone();
            tokio::spawn(async move {
                let mut buf = Vec::new();
                let mut chunk = [0u8; 8192];
                let head_end = loop {
                    let n = sock.read(&mut chunk).await.unwrap_or(0);
                    if n == 0 {
                        return;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                    if let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        break p + 4;
                    }
                };
                let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
                let mut lines = head.lines();
                let first = lines.next().unwrap_or("");
                let mut parts = first.split(' ');
                let method = parts.next().unwrap_or("").to_owned();
                let path = parts.next().unwrap_or("/").to_owned();
                let headers: Vec<(String, String)> = lines
                    .filter_map(|l| l.split_once(':'))
                    .map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned()))
                    .collect();
                let len = headers
                    .iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                    .and_then(|(_, v)| v.parse::<usize>().ok())
                    .unwrap_or(0);
                while buf.len() < head_end + len {
                    let n = sock.read(&mut chunk).await.unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                }
                let text = String::from_utf8_lossy(&buf[head_end..]).into_owned();
                let body = serde_json::from_str(&text).unwrap_or(Value::Null);
                let req = Req {
                    method,
                    path,
                    text,
                    headers,
                    body,
                };
                log.lock().expect("lock").push(req.clone());
                let reply = handler(&req);
                let mut out = format!(
                    "HTTP/1.1 {}\r\ncontent-type: {}\r\nconnection: close\r\ncontent-length: {}\r\n",
                    reply.status,
                    reply.content_type,
                    reply.body.len()
                );
                for (k, v) in &reply.headers {
                    out.push_str(&format!("{k}: {v}\r\n"));
                }
                out.push_str("\r\n");
                out.push_str(&reply.body);
                let _ = sock.write_all(out.as_bytes()).await;
                let _ = sock.shutdown().await;
            });
        }
    });
    TestServer {
        url: format!("http://{addr}/mcp"),
        origin: format!("http://{addr}"),
        log,
    }
}
