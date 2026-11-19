//! A tiny HTTP server for tests: serves canned replies in order and records the requests.

use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

/// A canned response.
pub struct Reply {
    status: String,
    content_type: &'static str,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    /// Stream the body without a content length (closing the connection ends it).
    streamed: bool,
}

impl Reply {
    pub fn sse(body: &str) -> Self {
        Self {
            status: "200 OK".into(),
            content_type: "text/event-stream",
            headers: Vec::new(),
            body: body.as_bytes().to_vec(),
            streamed: true,
        }
    }

    pub fn json(body: &str) -> Self {
        Self::status("200 OK", body)
    }

    pub fn status(status: &str, body: &str) -> Self {
        Self {
            status: status.into(),
            content_type: "application/json",
            headers: Vec::new(),
            body: body.as_bytes().to_vec(),
            streamed: false,
        }
    }

    /// A binary body (audio, video, images).
    pub fn bytes(content_type: &'static str, body: &[u8]) -> Self {
        Self {
            status: "200 OK".into(),
            content_type,
            headers: Vec::new(),
            body: body.to_vec(),
            streamed: false,
        }
    }

    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }
}

/// A request as received: the head (request line + headers) and the body.
#[derive(Debug, Clone)]
pub struct Received {
    pub head: String,
    pub body: String,
}

pub struct Server {
    /// Base URL ending at `/v1`.
    pub base: String,
    received: Arc<Mutex<Vec<Received>>>,
}

impl Server {
    pub async fn requests(&self) -> Vec<Received> {
        self.received.lock().await.clone()
    }
}

/// Serves `replies` to consecutive connections (one request per connection).
pub async fn serve(replies: Vec<Reply>) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("binds");
    let addr = listener.local_addr().expect("addr");
    let received = Arc::new(Mutex::new(Vec::new()));
    let log = received.clone();
    tokio::spawn(async move {
        for reply in replies {
            let Ok((mut sock, _)) = listener.accept().await else {
                return;
            };
            let req = read_request(&mut sock).await;
            log.lock().await.push(req);
            write_reply(&mut sock, &reply).await;
        }
    });
    Server {
        base: format!("http://{addr}/v1"),
        received,
    }
}

async fn read_request(sock: &mut TcpStream) -> Received {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        let n = sock.read(&mut chunk).await.unwrap_or(0);
        if n == 0 {
            break buf.len();
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break p + 4;
        }
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
    let length = head
        .lines()
        .find_map(|l| {
            let (k, v) = l.split_once(':')?;
            k.eq_ignore_ascii_case("content-length")
                .then(|| v.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0);
    while buf.len() < head_end + length {
        let n = sock.read(&mut chunk).await.unwrap_or(0);
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    let body = String::from_utf8_lossy(&buf[head_end..]).into_owned();
    Received { head, body }
}

async fn write_reply(sock: &mut TcpStream, reply: &Reply) {
    let mut head = format!(
        "HTTP/1.1 {}\r\ncontent-type: {}\r\nconnection: close\r\n",
        reply.status, reply.content_type
    );
    for (k, v) in &reply.headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    if !reply.streamed {
        head.push_str(&format!("content-length: {}\r\n", reply.body.len()));
    }
    head.push_str("\r\n");
    sock.write_all(head.as_bytes()).await.ok();
    if reply.streamed {
        // Send in small pieces so clients see events split across chunks.
        for piece in reply.body.chunks(37) {
            sock.write_all(piece).await.ok();
            sock.flush().await.ok();
        }
    } else {
        sock.write_all(&reply.body).await.ok();
    }
    sock.shutdown().await.ok();
}
