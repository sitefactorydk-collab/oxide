//! Oxide's own log (Phase 18): every line goes to stderr and, once the app's log folder is
//! known, to `<logs>/oxide.log` — with anything that looks like a secret replaced first
//! (`oxide_core::redact`). The file starts over when it passes 5 MB at start-up.

use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;
use std::time::Instant;

use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::MakeWriter;

const MAX_LOG_BYTES: u64 = 5 * 1024 * 1024;

static FILE: Mutex<Option<File>> = Mutex::new(None);
static STARTED: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

/// When `run` began (for the start-up time).
pub fn started() -> Instant {
    *STARTED.get_or_init(Instant::now)
}

fn lock() -> std::sync::MutexGuard<'static, Option<File>> {
    FILE.lock().unwrap_or_else(|e| e.into_inner())
}

/// One log event, collected and redacted as a whole when dropped.
pub struct EventWriter(Vec<u8>);

impl Write for EventWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Drop for EventWriter {
    fn drop(&mut self) {
        if self.0.is_empty() {
            return;
        }
        let text = String::from_utf8_lossy(&self.0);
        let clean = oxide_core::redact::redact_secrets(&text);
        let _ = std::io::stderr().write_all(clean.as_bytes());
        if let Some(f) = lock().as_mut() {
            let _ = f.write_all(clean.as_bytes());
        }
    }
}

#[derive(Clone, Copy)]
pub struct Redacting;

impl<'a> MakeWriter<'a> for Redacting {
    type Writer = EventWriter;
    fn make_writer(&'a self) -> Self::Writer {
        EventWriter(Vec::with_capacity(256))
    }
}

pub fn init() {
    started();
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,oxide=debug,oxide_lib=debug"));
    // Ignore the error if a subscriber is already set (e.g. in tests).
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(Redacting)
        .with_ansi(false)
        .try_init();
}

/// Starts writing to `<dir>/oxide.log` as well.
pub fn open_file(dir: &Path) {
    let path = dir.join("oxide.log");
    if let Err(e) = std::fs::create_dir_all(dir) {
        tracing::warn!("no log folder: {e}");
        return;
    }
    let too_big = std::fs::metadata(&path)
        .map(|m| m.len() > MAX_LOG_BYTES)
        .unwrap_or(false);
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(!too_big)
        .write(true)
        .truncate(too_big)
        .open(&path);
    match file {
        Ok(f) => *lock() = Some(f),
        Err(e) => tracing::warn!("couldn't open {}: {e}", path.display()),
    }
}

/// Stops writing to the file (before "Delete all" empties the log folder).
pub fn close_file() {
    *lock() = None;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_are_redacted_before_they_are_written() {
        let dir = std::env::temp_dir().join(format!("oxide-log-test-{}", std::process::id()));
        open_file(&dir);
        {
            let mut w = Redacting.make_writer();
            let _ = w.write_all(
                b"provider said: invalid x-api-key: sk-ant-api03-abcdefghijklmnopqrstu\n",
            );
        }
        *lock() = None;
        let text = std::fs::read_to_string(dir.join("oxide.log")).expect("log");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(text.contains("x-api-key: [redacted]"), "{text}");
        assert!(!text.contains("abcdefghijklmnop"));
    }
}
