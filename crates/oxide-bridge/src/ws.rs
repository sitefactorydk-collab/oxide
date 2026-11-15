//! A small WebSocket (RFC 6455) implementation: the server handshake and the frame codec.
//!
//! Only what the bridge needs: text messages (JSON-RPC), ping/pong and close. Frames from
//! clients must be masked (the RFC requires it); fragmented messages are joined; messages
//! larger than a limit end the connection.

use base64::Engine as _;
use sha1::{Digest, Sha1};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// The GUID of RFC 6455, section 1.3.
const WS_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
/// Longest request head a client may send.
const MAX_HEAD: usize = 8 * 1024;

pub const OP_CONT: u8 = 0x0;
pub const OP_TEXT: u8 = 0x1;
pub const OP_BINARY: u8 = 0x2;
pub const OP_CLOSE: u8 = 0x8;
pub const OP_PING: u8 = 0x9;
pub const OP_PONG: u8 = 0xA;

#[derive(Debug, thiserror::Error)]
pub enum WsError {
    #[error("connection error: {0}")]
    Io(#[from] std::io::Error),
    #[error("protocol error: {0}")]
    Protocol(&'static str),
    #[error("message too large")]
    TooLarge,
    #[error("the connection closed")]
    Closed,
}

/// `Sec-WebSocket-Accept` for a client's `Sec-WebSocket-Key`.
pub fn accept_key(key: &str) -> String {
    let mut h = Sha1::new();
    h.update(key.trim().as_bytes());
    h.update(WS_GUID.as_bytes());
    base64::engine::general_purpose::STANDARD.encode(h.finalize())
}

/// A parsed HTTP request head.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RequestHead {
    pub method: String,
    pub path: String,
    /// Header names in lower case.
    pub headers: Vec<(String, String)>,
}

impl RequestHead {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// Whether a comma-separated header holds `token` (case-insensitive).
    pub fn header_has(&self, name: &str, token: &str) -> bool {
        self.header(name)
            .is_some_and(|v| v.split(',').any(|t| t.trim().eq_ignore_ascii_case(token)))
    }
}

/// Reads an HTTP request head (up to the blank line). `None` when it is malformed or too
/// long. Bytes after the head are not read (clients wait for the 101 before sending).
pub async fn read_head<R: AsyncRead + Unpin>(r: &mut R) -> Result<Option<RequestHead>, WsError> {
    let mut buf = Vec::with_capacity(1024);
    let mut byte = [0u8; 1];
    while !buf.ends_with(b"\r\n\r\n") {
        if buf.len() >= MAX_HEAD {
            return Ok(None);
        }
        if r.read(&mut byte).await? == 0 {
            return Err(WsError::Closed);
        }
        buf.push(byte[0]);
    }
    Ok(parse_head(&buf))
}

pub fn parse_head(buf: &[u8]) -> Option<RequestHead> {
    let text = std::str::from_utf8(buf).ok()?;
    let mut lines = text.split("\r\n");
    let mut first = lines.next()?.split(' ');
    let method = first.next()?.to_owned();
    let path = first.next()?.to_owned();
    if !first.next()?.starts_with("HTTP/1.1") {
        return None;
    }
    let mut headers = Vec::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        let (k, v) = line.split_once(':')?;
        headers.push((k.trim().to_ascii_lowercase(), v.trim().to_owned()));
    }
    Some(RequestHead {
        method,
        path,
        headers,
    })
}

/// A plain HTTP answer that refuses the upgrade, then the connection closes.
pub fn refusal(status: u16, reason: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/plain; charset=utf-8\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

/// The 101 answer that accepts the upgrade.
pub fn switching(key: &str) -> String {
    format!(
        "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
         Sec-WebSocket-Accept: {}\r\n\r\n",
        accept_key(key)
    )
}

/// One message (or control frame) read from the peer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    Text(String),
    Binary(Vec<u8>),
    Ping(Vec<u8>),
    Pong(Vec<u8>),
    /// Code and reason, when given.
    Close(Option<(u16, String)>),
}

/// Reads messages, joining fragments. Not cancel-safe: run it in a task of its own.
pub struct Reader<R> {
    inner: R,
    /// Frames from the peer must be masked (true on the server side).
    expect_mask: bool,
    max: usize,
    partial: Option<(u8, Vec<u8>)>,
}

impl<R: AsyncRead + Unpin> Reader<R> {
    pub fn new(inner: R, expect_mask: bool, max: usize) -> Self {
        Self {
            inner,
            expect_mask,
            max,
            partial: None,
        }
    }

    pub async fn next(&mut self) -> Result<Message, WsError> {
        loop {
            let mut h = [0u8; 2];
            match self.inner.read_exact(&mut h).await {
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    return Err(WsError::Closed);
                }
                Err(e) => return Err(e.into()),
            }
            let fin = h[0] & 0x80 != 0;
            if h[0] & 0x70 != 0 {
                return Err(WsError::Protocol("reserved bits set"));
            }
            let op = h[0] & 0x0F;
            let masked = h[1] & 0x80 != 0;
            if masked != self.expect_mask {
                return Err(WsError::Protocol(if self.expect_mask {
                    "client frames must be masked"
                } else {
                    "server frames must not be masked"
                }));
            }
            let len = match h[1] & 0x7F {
                126 => {
                    let mut b = [0u8; 2];
                    self.inner.read_exact(&mut b).await?;
                    u64::from(u16::from_be_bytes(b))
                }
                127 => {
                    let mut b = [0u8; 8];
                    self.inner.read_exact(&mut b).await?;
                    u64::from_be_bytes(b)
                }
                n => u64::from(n),
            };
            let control = op & 0x8 != 0;
            if control && (len > 125 || !fin) {
                return Err(WsError::Protocol("bad control frame"));
            }
            let have = self.partial.as_ref().map_or(0, |(_, b)| b.len());
            let len = usize::try_from(len).map_err(|_| WsError::TooLarge)?;
            if have.saturating_add(len) > self.max {
                return Err(WsError::TooLarge);
            }
            let mut key = [0u8; 4];
            if masked {
                self.inner.read_exact(&mut key).await?;
            }
            let mut payload = vec![0u8; len];
            self.inner.read_exact(&mut payload).await?;
            if masked {
                for (i, b) in payload.iter_mut().enumerate() {
                    *b ^= key[i % 4];
                }
            }
            match op {
                OP_PING => return Ok(Message::Ping(payload)),
                OP_PONG => return Ok(Message::Pong(payload)),
                OP_CLOSE => {
                    let close = if payload.len() >= 2 {
                        let code = u16::from_be_bytes([payload[0], payload[1]]);
                        let reason = String::from_utf8_lossy(&payload[2..]).into_owned();
                        Some((code, reason))
                    } else {
                        None
                    };
                    return Ok(Message::Close(close));
                }
                OP_TEXT | OP_BINARY => {
                    if self.partial.is_some() {
                        return Err(WsError::Protocol("new message inside a fragmented one"));
                    }
                    if fin {
                        return finish(op, payload);
                    }
                    self.partial = Some((op, payload));
                }
                OP_CONT => {
                    let Some((first, mut buf)) = self.partial.take() else {
                        return Err(WsError::Protocol("continuation without a message"));
                    };
                    buf.extend_from_slice(&payload);
                    if fin {
                        return finish(first, buf);
                    }
                    self.partial = Some((first, buf));
                }
                _ => return Err(WsError::Protocol("unknown opcode")),
            }
        }
    }
}

fn finish(op: u8, payload: Vec<u8>) -> Result<Message, WsError> {
    if op == OP_TEXT {
        String::from_utf8(payload)
            .map(Message::Text)
            .map_err(|_| WsError::Protocol("text is not UTF-8"))
    } else {
        Ok(Message::Binary(payload))
    }
}

/// Encodes one unfragmented frame. `mask` is for clients.
pub fn encode(op: u8, payload: &[u8], mask: Option<[u8; 4]>) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() + 14);
    out.push(0x80 | op);
    let m = if mask.is_some() { 0x80 } else { 0 };
    let len = payload.len();
    if len < 126 {
        // Fits in 7 bits (checked above).
        out.push(m | u8::try_from(len).unwrap_or(125));
    } else if let Ok(l) = u16::try_from(len) {
        out.push(m | 126);
        out.extend_from_slice(&l.to_be_bytes());
    } else {
        out.push(m | 127);
        out.extend_from_slice(&(len as u64).to_be_bytes());
    }
    match mask {
        Some(key) => {
            out.extend_from_slice(&key);
            out.extend(payload.iter().enumerate().map(|(i, b)| b ^ key[i % 4]));
        }
        None => out.extend_from_slice(payload),
    }
    out
}

/// A close frame's payload.
pub fn close_payload(code: u16, reason: &str) -> Vec<u8> {
    let mut p = code.to_be_bytes().to_vec();
    p.extend(reason.as_bytes().iter().take(120));
    p
}

/// Writes one frame and flushes.
pub async fn write_frame<W: AsyncWrite + Unpin>(
    w: &mut W,
    op: u8,
    payload: &[u8],
    mask: Option<[u8; 4]>,
) -> Result<(), WsError> {
    w.write_all(&encode(op, payload, mask)).await?;
    w.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accept_key_matches_the_rfc_example() {
        assert_eq!(
            accept_key("dGhlIHNhbXBsZSBub25jZQ=="),
            "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
        );
    }

    #[test]
    fn heads_parse_and_headers_are_case_insensitive() {
        let h = parse_head(
            b"GET / HTTP/1.1\r\nHost: 127.0.0.1:1\r\nConnection: keep-alive, Upgrade\r\n\
              Upgrade: WebSocket\r\n\r\n",
        )
        .expect("parses");
        assert_eq!(h.method, "GET");
        assert_eq!(h.header("HOST"), Some("127.0.0.1:1"));
        assert!(h.header_has("connection", "upgrade"));
        assert!(h.header_has("upgrade", "websocket"));
        assert!(parse_head(b"GET / HTTP/1.0\r\n\r\n").is_none());
        assert!(parse_head(b"nonsense\r\n\r\n").is_none());
    }

    async fn read_all(bytes: Vec<u8>, mask: bool, max: usize) -> Vec<Result<Message, String>> {
        let mut r = Reader::new(std::io::Cursor::new(bytes), mask, max);
        let mut out = Vec::new();
        loop {
            match r.next().await {
                Ok(m) => out.push(Ok(m)),
                Err(WsError::Closed) => break,
                Err(e) => {
                    out.push(Err(e.to_string()));
                    break;
                }
            }
        }
        out
    }

    #[tokio::test]
    async fn frames_round_trip_with_all_length_forms() {
        let key = Some([1, 2, 3, 4]);
        let mut bytes = Vec::new();
        for n in [0usize, 5, 125, 126, 65_535, 65_536, 200_000] {
            bytes.extend(encode(OP_TEXT, "x".repeat(n).as_bytes(), key));
        }
        let got = read_all(bytes, true, 1 << 20).await;
        let lens: Vec<usize> = got
            .iter()
            .map(|m| match m {
                Ok(Message::Text(t)) => t.len(),
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(lens, vec![0, 5, 125, 126, 65_535, 65_536, 200_000]);
    }

    #[tokio::test]
    async fn fragments_are_joined_around_control_frames() {
        let key = Some([9, 8, 7, 6]);
        let mut bytes = Vec::new();
        // "hel" (not fin), ping, "lo" (fin continuation).
        let mut first = encode(OP_TEXT, b"hel", key);
        first[0] &= 0x7F;
        bytes.extend(first);
        bytes.extend(encode(OP_PING, b"p", key));
        bytes.extend(encode(OP_CONT, b"lo", key));
        bytes.extend(encode(OP_CLOSE, &close_payload(1000, "bye"), key));
        let got = read_all(bytes, true, 1 << 20).await;
        assert_eq!(
            got,
            vec![
                Ok(Message::Ping(b"p".to_vec())),
                Ok(Message::Text("hello".into())),
                Ok(Message::Close(Some((1000, "bye".into())))),
            ]
        );
    }

    #[tokio::test]
    async fn unmasked_oversized_and_broken_frames_are_refused() {
        let got = read_all(encode(OP_TEXT, b"hi", None), true, 100).await;
        assert!(matches!(&got[0], Err(e) if e.contains("masked")), "{got:?}");
        let got = read_all(encode(OP_TEXT, &[b'a'; 200], Some([0; 4])), true, 100).await;
        assert!(
            matches!(&got[0], Err(e) if e.contains("too large")),
            "{got:?}"
        );
        let got = read_all(encode(OP_CONT, b"x", Some([0; 4])), true, 100).await;
        assert!(
            matches!(&got[0], Err(e) if e.contains("continuation")),
            "{got:?}"
        );
        let got = read_all(encode(OP_TEXT, &[0xff, 0xfe], Some([0; 4])), true, 100).await;
        assert!(matches!(&got[0], Err(e) if e.contains("UTF-8")), "{got:?}");
        let mut bad = encode(OP_TEXT, b"x", Some([0; 4]));
        bad[0] |= 0x40;
        let got = read_all(bad, true, 100).await;
        assert!(
            matches!(&got[0], Err(e) if e.contains("reserved")),
            "{got:?}"
        );
    }

    /// Random and mutated frames: the reader never panics, never returns more than its
    /// limit, and stops at the first error (Phase 18 fuzzing).
    #[tokio::test]
    async fn random_and_mutated_frames_never_panic() {
        use rand::{Rng, SeedableRng};
        let mut rng = rand::rngs::StdRng::seed_from_u64(0x0_51de);
        let max = 4_096;
        for round in 0..4_000 {
            let mut bytes: Vec<u8> = if round % 2 == 0 {
                // Pure noise.
                (0..rng.gen_range(0..300)).map(|_| rng.r#gen()).collect()
            } else {
                // Valid frames with a few bits flipped and a random cut.
                let mut b = Vec::new();
                for _ in 0..rng.gen_range(1..4) {
                    let op = [OP_TEXT, OP_CONT, OP_PING, OP_CLOSE, OP_BINARY][rng.gen_range(0..5)];
                    let n = rng.gen_range(0..6_000);
                    let payload: Vec<u8> = (0..n).map(|_| rng.gen_range(b'a'..=b'z')).collect();
                    let mask = if rng.gen_bool(0.9) { Some(rng.r#gen()) } else { None };
                    b.extend(encode(op, &payload, mask));
                }
                for _ in 0..rng.gen_range(0..4) {
                    let at = rng.gen_range(0..b.len());
                    b[at] ^= 1 << rng.gen_range(0..8);
                }
                let cut = rng.gen_range(0..=b.len());
                b.truncate(cut);
                b
            };
            // Huge declared lengths must not allocate their size.
            if round % 97 == 0 {
                bytes = vec![0x81, 0xFF, 0x7F, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 1, 2, 3, 4];
            }
            let got = read_all(bytes, true, max).await;
            for (k, m) in got.iter().enumerate() {
                match m {
                    Ok(Message::Text(t)) => assert!(t.len() <= max),
                    Ok(_) => {}
                    Err(_) => assert_eq!(k, got.len() - 1, "reading stops at an error"),
                }
            }
        }
    }
}
