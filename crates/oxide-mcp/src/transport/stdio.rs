//! stdio: the server is a child process; one JSON-RPC message per line on stdin/stdout,
//! stderr goes to the server's log.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use oxide_core::McpLogSource;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, oneshot, watch};

use super::{InboundTx, Pending, PendingGuard, ServerMessage};
use crate::McpError;
use crate::jsonrpc::{self, Incoming, RequestId, RpcError};
use crate::log::ServerLog;
use crate::process::{self, ProcessSpec};

/// How long a server gets to exit after its stdin closes before it is killed.
const EXIT_GRACE: Duration = Duration::from_secs(2);

pub struct StdioTransport {
    stdin: Mutex<Option<mpsc::UnboundedSender<String>>>,
    pending: Pending,
    closed: watch::Receiver<Option<String>>,
    kill: Mutex<Option<oneshot::Sender<()>>>,
    pid: Option<u32>,
}

impl StdioTransport {
    /// Starts the process and its reader, writer and stderr tasks.
    pub fn spawn(
        spec: &ProcessSpec,
        inbound: InboundTx,
        log: Arc<ServerLog>,
    ) -> Result<Self, McpError> {
        let process::Spawned {
            mut child,
            pid,
            tree,
        } = process::spawn(spec)?;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let (Some(mut stdin), Some(stdout), Some(stderr)) = (stdin, stdout, stderr) else {
            return Err(McpError::Spawn("the process has no stdio pipes".into()));
        };
        log.oxide(&format!(
            "started {} {} (pid {})",
            spec.command,
            spec.args.join(" "),
            pid.map(|p| p.to_string()).unwrap_or_else(|| "?".into())
        ));

        let pending = Pending::default();
        let (closed_tx, closed) = watch::channel(None::<String>);

        // Writer: lines to stdin. Dropping the sender closes stdin.
        let (line_tx, mut line_rx) = mpsc::unbounded_channel::<String>();
        tokio::spawn(async move {
            while let Some(line) = line_rx.recv().await {
                if stdin.write_all(line.as_bytes()).await.is_err()
                    || stdin.write_all(b"\n").await.is_err()
                    || stdin.flush().await.is_err()
                {
                    break;
                }
            }
            let _ = stdin.shutdown().await;
        });

        // Reader: stdout lines → responses or inbound messages.
        let reader_pending = pending.clone();
        let reader_log = log.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            let mut buf = Vec::new();
            loop {
                buf.clear();
                match reader.read_until(b'\n', &mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
                let line = String::from_utf8_lossy(&buf);
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                match jsonrpc::parse_text(line) {
                    Ok(msgs) => {
                        for msg in msgs {
                            dispatch(msg, &reader_pending, &inbound);
                        }
                    }
                    Err(_) => reader_log.push(McpLogSource::Stderr, Some("stdout"), line),
                }
            }
            // No more answers can come.
            reader_pending.clear();
        });

        // stderr → log.
        let err_log = log.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                err_log.push(McpLogSource::Stderr, None, &line);
            }
        });

        // Waiter: reports the exit, or kills the tree when asked.
        let (kill_tx, kill_rx) = oneshot::channel::<()>();
        let wait_log = log;
        let wait_pending = pending.clone();
        tokio::spawn(async move {
            let status = tokio::select! {
                s = child.wait() => s,
                _ = kill_rx => {
                    tree.kill();
                    let _ = child.start_kill();
                    child.wait().await
                }
            };
            // Anything the server started in the background goes too.
            tree.kill();
            let reason = match status {
                Ok(s) => match s.code() {
                    Some(0) => " (exit code 0)".to_owned(),
                    Some(c) => format!(" (exit code {c})"),
                    None => " (killed)".to_owned(),
                },
                Err(e) => format!(" ({e})"),
            };
            wait_log.oxide(&format!("process ended{reason}"));
            wait_pending.clear();
            let _ = closed_tx.send(Some(reason));
        });

        Ok(Self {
            stdin: Mutex::new(Some(line_tx)),
            pending,
            closed,
            kill: Mutex::new(Some(kill_tx)),
            pid,
        })
    }

    pub fn pid(&self) -> Option<u32> {
        self.pid
    }

    fn send_line(&self, value: &Value) -> Result<(), McpError> {
        let line = value.to_string();
        let guard = self.stdin.lock().unwrap_or_else(|e| e.into_inner());
        match guard.as_ref() {
            Some(tx) if tx.send(line).is_ok() => Ok(()),
            _ => Err(self.closed_error()),
        }
    }

    fn closed_error(&self) -> McpError {
        McpError::Closed(self.closed.borrow().clone().unwrap_or_default())
    }

    pub async fn request(
        &self,
        id: &RequestId,
        method: &str,
        params: Value,
    ) -> Result<Value, McpError> {
        let rx = self.pending.add(id.clone());
        let _guard = PendingGuard {
            pending: self.pending.clone(),
            id: id.clone(),
        };
        self.send_line(&jsonrpc::request(id, method, &params))?;
        match rx.await {
            Ok(Ok(v)) => Ok(v),
            Ok(Err(e)) => Err(McpError::Rpc(e)),
            Err(_) => {
                // The reader cleared the waiters: give the waiter task a moment to record why.
                let mut closed = self.closed.clone();
                let _ = tokio::time::timeout(
                    Duration::from_millis(300),
                    closed.wait_for(Option::is_some),
                )
                .await;
                Err(self.closed_error())
            }
        }
    }

    pub async fn notify(&self, method: &str, params: Value) -> Result<(), McpError> {
        self.send_line(&jsonrpc::notification(method, &params))
    }

    pub async fn respond(
        &self,
        id: &RequestId,
        outcome: Result<Value, RpcError>,
    ) -> Result<(), McpError> {
        self.send_line(&jsonrpc::response(id, &outcome))
    }

    pub fn closed(&self) -> watch::Receiver<Option<String>> {
        self.closed.clone()
    }

    /// Closes stdin, waits briefly for the server to exit, then kills its process tree.
    pub async fn close(&self) {
        self.stdin.lock().unwrap_or_else(|e| e.into_inner()).take();
        let mut closed = self.closed.clone();
        let exited = tokio::time::timeout(EXIT_GRACE, closed.wait_for(Option::is_some))
            .await
            .is_ok();
        if !exited {
            if let Some(kill) = self.kill.lock().unwrap_or_else(|e| e.into_inner()).take() {
                let _ = kill.send(());
            }
            let _ = tokio::time::timeout(EXIT_GRACE, closed.wait_for(Option::is_some)).await;
        }
    }
}

impl Drop for StdioTransport {
    fn drop(&mut self) {
        if let Some(kill) = self.kill.lock().unwrap_or_else(|e| e.into_inner()).take() {
            let _ = kill.send(());
        }
    }
}

pub(crate) fn dispatch(msg: Incoming, pending: &Pending, inbound: &InboundTx) {
    match msg {
        Incoming::Response {
            id: Some(id),
            outcome,
        } => {
            if !pending.resolve(&id, outcome) {
                tracing::debug!("MCP response for unknown request {id}");
            }
        }
        Incoming::Response { id: None, outcome } => {
            tracing::debug!("MCP error without an id: {outcome:?}");
        }
        Incoming::Request { id, method, params } => {
            let _ = inbound.send(ServerMessage::Request { id, method, params });
        }
        Incoming::Notification { method, params } => {
            let _ = inbound.send(ServerMessage::Notification { method, params });
        }
    }
}
