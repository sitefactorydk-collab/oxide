//! Per-server logs: the last lines in memory for the log viewer, everything appended to
//! `<logs dir>/mcp-<name>.log`, and each line forwarded to a listener (the UI). Secrets are
//! removed from every line first (`oxide_core::redact`).

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use oxide_core::{McpLogLine, McpLogSource};
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;

/// Lines kept in memory per server.
const MAX_LINES: usize = 1_000;
/// Longest line kept; longer ones are cut.
const MAX_LINE_CHARS: usize = 4_000;
/// A log file bigger than this is started over when the server starts.
const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;

pub type LogListener = Arc<dyn Fn(McpLogLine) + Send + Sync>;

pub struct ServerLog {
    lines: Mutex<VecDeque<McpLogLine>>,
    file: Option<mpsc::UnboundedSender<String>>,
    listener: Option<LogListener>,
}

impl ServerLog {
    /// A log that writes to `file` (if any) and calls `listener` for every line.
    pub fn new(file: Option<PathBuf>, listener: Option<LogListener>) -> Arc<Self> {
        let file = file.and_then(|path| {
            // Writing needs a runtime; tests without one just keep lines in memory.
            let handle = tokio::runtime::Handle::try_current().ok()?;
            let (tx, rx) = mpsc::unbounded_channel::<String>();
            handle.spawn(write_file(path, rx));
            Some(tx)
        });
        Arc::new(Self {
            lines: Mutex::new(VecDeque::new()),
            file,
            listener,
        })
    }

    pub fn memory() -> Arc<Self> {
        Self::new(None, None)
    }

    pub fn push(&self, source: McpLogSource, level: Option<&str>, text: &str) {
        // Servers print what they like (tokens in URLs, keys in errors): keep secrets out of
        // the viewer and the file.
        let text = oxide_core::redact::redact_secrets(text.trim_end());
        let text = text.as_ref();
        let text: String = if text.chars().count() > MAX_LINE_CHARS {
            let mut t: String = text.chars().take(MAX_LINE_CHARS).collect();
            t.push('…');
            t
        } else {
            text.to_owned()
        };
        let line = McpLogLine {
            at: now_ms(),
            source,
            level: level.map(str::to_owned),
            text,
        };
        if let Some(tx) = &self.file {
            let tag = match source {
                McpLogSource::Stderr => "stderr",
                McpLogSource::Server => "server",
                McpLogSource::Oxide => "oxide",
            };
            let level = line
                .level
                .as_deref()
                .map(|l| format!(" {l}"))
                .unwrap_or_default();
            let _ = tx.send(format!("{} [{tag}{level}] {}\n", line.at, line.text));
        }
        {
            let mut lines = self.lines.lock().unwrap_or_else(|e| e.into_inner());
            if lines.len() == MAX_LINES {
                lines.pop_front();
            }
            lines.push_back(line.clone());
        }
        if let Some(l) = &self.listener {
            l(line);
        }
    }

    pub fn oxide(&self, text: &str) {
        self.push(McpLogSource::Oxide, None, text);
    }

    pub fn lines(&self) -> Vec<McpLogLine> {
        self.lines
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .cloned()
            .collect()
    }
}

async fn write_file(path: PathBuf, mut rx: mpsc::UnboundedReceiver<String>) {
    if let Some(dir) = path.parent() {
        let _ = tokio::fs::create_dir_all(dir).await;
    }
    let too_big = tokio::fs::metadata(&path)
        .await
        .map(|m| m.len() > MAX_FILE_BYTES)
        .unwrap_or(false);
    let file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(!too_big)
        .write(true)
        .truncate(too_big)
        .open(&path)
        .await;
    let Ok(mut file) = file else {
        tracing::warn!("couldn't open MCP log {}", path.display());
        return;
    };
    while let Some(line) = rx.recv().await {
        if file.write_all(line.as_bytes()).await.is_err() {
            return;
        }
    }
    let _ = file.flush().await;
}

pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// A file name for a server's log.
pub fn log_file_name(server: &str) -> String {
    let safe: String = server
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .take(60)
        .collect();
    format!("mcp-{safe}.log")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_last_lines_and_cuts_long_ones() {
        let log = ServerLog::memory();
        for i in 0..(MAX_LINES + 5) {
            log.push(McpLogSource::Stderr, None, &format!("line {i}"));
        }
        let lines = log.lines();
        assert_eq!(lines.len(), MAX_LINES);
        assert_eq!(lines[0].text, "line 5");
        log.oxide(&"x".repeat(MAX_LINE_CHARS + 10));
        let last = log.lines().pop().expect("line");
        assert_eq!(last.text.chars().count(), MAX_LINE_CHARS + 1);
    }

    #[test]
    fn file_names_are_safe() {
        assert_eq!(log_file_name("my server/../x"), "mcp-my_server____x.log");
    }

    #[tokio::test]
    async fn lines_reach_the_file_and_the_listener() {
        let dir = tempfile::tempdir().expect("tmp");
        let path = dir.path().join("logs").join("mcp-a.log");
        let seen = Arc::new(Mutex::new(0));
        let s = seen.clone();
        let log = ServerLog::new(
            Some(path.clone()),
            Some(Arc::new(move |_| *s.lock().expect("lock") += 1)),
        );
        log.push(McpLogSource::Server, Some("info"), "hello");
        log.oxide("bye");
        drop(log);
        for _ in 0..50 {
            if std::fs::read_to_string(&path).is_ok_and(|t| t.contains("bye")) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        let text = std::fs::read_to_string(&path).expect("log file");
        assert!(text.contains("[server info] hello"), "{text}");
        assert_eq!(*seen.lock().expect("lock"), 2);
    }
}
