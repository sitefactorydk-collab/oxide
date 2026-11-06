//! A small, incremental Server-Sent Events parser (the same one `oxide-llm` uses; kept
//! separate so the MCP client doesn't depend on the model layer).
//!
//! Bytes arrive in arbitrary chunks; lines (and multi-byte UTF-8 characters) can be split
//! across them. Only complete lines are decoded.

/// One dispatched event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseEvent {
    /// The `event:` field, if any.
    pub event: Option<String>,
    /// The `data:` lines joined with `\n`.
    pub data: String,
}

#[derive(Debug, Default)]
pub struct SseParser {
    buf: Vec<u8>,
    event: Option<String>,
    data: String,
    has_data: bool,
}

impl SseParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds a chunk and returns the events it completed.
    pub fn push(&mut self, chunk: &[u8]) -> Vec<SseEvent> {
        self.buf.extend_from_slice(chunk);
        let mut out = Vec::new();
        while let Some(pos) = self.buf.iter().position(|&b| b == b'\n') {
            let mut line: Vec<u8> = self.buf.drain(..=pos).collect();
            line.pop(); // '\n'
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            let line = String::from_utf8_lossy(&line).into_owned();
            self.line(&line, &mut out);
        }
        out
    }

    /// Ends the input: a final event without a trailing blank line is still dispatched.
    pub fn finish(&mut self) -> Option<SseEvent> {
        if !self.buf.is_empty() {
            let rest = std::mem::take(&mut self.buf);
            let line = String::from_utf8_lossy(&rest)
                .trim_end_matches('\r')
                .to_owned();
            let mut out = Vec::new();
            self.line(&line, &mut out);
            if let Some(ev) = out.pop() {
                return Some(ev);
            }
        }
        self.dispatch()
    }

    fn line(&mut self, line: &str, out: &mut Vec<SseEvent>) {
        if line.is_empty() {
            if let Some(ev) = self.dispatch() {
                out.push(ev);
            }
            return;
        }
        if line.starts_with(':') {
            return; // comment / keep-alive
        }
        let (field, value) = match line.split_once(':') {
            Some((f, v)) => (f, v.strip_prefix(' ').unwrap_or(v)),
            None => (line, ""),
        };
        match field {
            "event" => self.event = Some(value.to_owned()),
            "data" => {
                if self.has_data {
                    self.data.push('\n');
                }
                self.data.push_str(value);
                self.has_data = true;
            }
            _ => {} // id, retry: not needed
        }
    }

    fn dispatch(&mut self) -> Option<SseEvent> {
        let event = self.event.take();
        if !self.has_data {
            return None;
        }
        self.has_data = false;
        Some(SseEvent {
            event,
            data: std::mem::take(&mut self.data),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_named_events_and_multiline_data() {
        let mut p = SseParser::new();
        let evs = p.push(b"event: ping\ndata: {}\n\n: keep-alive\ndata: a\ndata: b\n\n");
        assert_eq!(
            evs,
            vec![
                SseEvent {
                    event: Some("ping".into()),
                    data: "{}".into()
                },
                SseEvent {
                    event: None,
                    data: "a\nb".into()
                },
            ]
        );
    }

    #[test]
    fn handles_crlf_and_chunks_split_anywhere() {
        let input = "event: x\r\ndata: h\u{e6}llo \u{1F980}\r\n\r\ndata: [DONE]\r\n\r\n".as_bytes();
        for split in 1..input.len() {
            let mut p = SseParser::new();
            let mut evs = p.push(&input[..split]);
            evs.extend(p.push(&input[split..]));
            assert_eq!(evs.len(), 2, "split at {split}");
            assert_eq!(evs[0].data, "h\u{e6}llo \u{1F980}");
            assert_eq!(evs[0].event.as_deref(), Some("x"));
            assert_eq!(evs[1].data, "[DONE]");
        }
    }

    #[test]
    fn finish_flushes_an_unterminated_event() {
        let mut p = SseParser::new();
        assert!(p.push(b"data: last").is_empty());
        assert_eq!(p.finish().map(|e| e.data), Some("last".into()));
        assert_eq!(p.finish(), None);
    }

    #[test]
    fn events_without_data_are_dropped() {
        let mut p = SseParser::new();
        assert!(p.push(b"event: empty\n\n").is_empty());
    }
}
