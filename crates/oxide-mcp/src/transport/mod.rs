//! Transports carry JSON-RPC messages to a server and bring back its answers.
//!
//! Both kinds work the same way for the client: [`Transport::request`] sends one request and
//! resolves with its response; anything else the server sends meanwhile (notifications, and
//! requests from legacy servers) goes to the inbound channel given at creation.

pub mod http;
pub mod stdio;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use oxide_core::McpEra;
use serde_json::Value;
use tokio::sync::{mpsc, oneshot, watch};

use crate::McpError;
use crate::jsonrpc::{RequestId, RpcError};

pub use http::{AuthProvider, HttpConfig, HttpTransport};
pub use stdio::StdioTransport;

/// A message the server sent on its own.
#[derive(Debug, Clone, PartialEq)]
pub enum ServerMessage {
    Request {
        id: RequestId,
        method: String,
        params: Value,
    },
    Notification {
        method: String,
        params: Value,
    },
}

pub type InboundTx = mpsc::UnboundedSender<ServerMessage>;

/// Per-request settings.
#[derive(Debug, Clone, Default)]
pub struct RequestOptions {
    /// `None` waits for as long as it takes (subscriptions).
    pub timeout: Option<Duration>,
    /// Extra HTTP headers (`Mcp-Param-*`).
    pub headers: Vec<(String, String)>,
}

type Waiter = oneshot::Sender<Result<Value, RpcError>>;

/// Requests waiting for their response, by id.
#[derive(Clone, Default)]
pub(crate) struct Pending(Arc<Mutex<HashMap<RequestId, Waiter>>>);

impl Pending {
    fn map(&self) -> std::sync::MutexGuard<'_, HashMap<RequestId, Waiter>> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub(crate) fn add(&self, id: RequestId) -> oneshot::Receiver<Result<Value, RpcError>> {
        let (tx, rx) = oneshot::channel();
        self.map().insert(id, tx);
        rx
    }

    pub(crate) fn resolve(&self, id: &RequestId, outcome: Result<Value, RpcError>) -> bool {
        match self.map().remove(id) {
            Some(tx) => {
                let _ = tx.send(outcome);
                true
            }
            None => false,
        }
    }

    pub(crate) fn remove(&self, id: &RequestId) {
        self.map().remove(id);
    }

    /// Drops every waiter (they see the connection as closed).
    pub(crate) fn clear(&self) {
        self.map().clear();
    }
}

/// Removes a pending entry when the request future ends early (dropped or timed out).
pub(crate) struct PendingGuard {
    pub(crate) pending: Pending,
    pub(crate) id: RequestId,
}

impl Drop for PendingGuard {
    fn drop(&mut self) {
        self.pending.remove(&self.id);
    }
}

/// A connection to one server.
pub enum Transport {
    Stdio(StdioTransport),
    Http(HttpTransport),
}

impl Transport {
    pub async fn request(
        &self,
        id: &RequestId,
        method: &str,
        params: Value,
        opts: &RequestOptions,
    ) -> Result<Value, McpError> {
        let fut = async {
            match self {
                Transport::Stdio(t) => t.request(id, method, params).await,
                Transport::Http(t) => t.request(id, method, params, &opts.headers).await,
            }
        };
        match opts.timeout {
            Some(d) => tokio::time::timeout(d, fut)
                .await
                .map_err(|_| McpError::Timeout(d.as_secs()))?,
            None => fut.await,
        }
    }

    pub async fn notify(&self, method: &str, params: Value) -> Result<(), McpError> {
        match self {
            Transport::Stdio(t) => t.notify(method, params).await,
            Transport::Http(t) => t.notify(method, params).await,
        }
    }

    /// Answers a request the server sent (legacy servers only).
    pub async fn respond(
        &self,
        id: &RequestId,
        outcome: Result<Value, RpcError>,
    ) -> Result<(), McpError> {
        match self {
            Transport::Stdio(t) => t.respond(id, outcome).await,
            Transport::Http(t) => t.respond(id, outcome).await,
        }
    }

    /// Becomes `Some(reason)` once the connection is gone for good.
    pub fn closed(&self) -> watch::Receiver<Option<String>> {
        match self {
            Transport::Stdio(t) => t.closed(),
            Transport::Http(t) => t.closed(),
        }
    }

    /// Records the negotiated era and version (HTTP sends them as headers).
    pub fn set_protocol(&self, era: McpEra, version: &str) {
        if let Transport::Http(t) = self {
            t.set_protocol(era, version);
        }
    }

    /// Whether cancelling a request needs a `notifications/cancelled` message. Modern HTTP
    /// cancels by closing the response stream instead.
    pub fn sends_cancel_notifications(&self, era: McpEra) -> bool {
        match self {
            Transport::Stdio(_) => true,
            Transport::Http(_) => era == McpEra::Legacy,
        }
    }

    pub fn is_http(&self) -> bool {
        matches!(self, Transport::Http(_))
    }

    /// Legacy HTTP: opens the standalone GET stream for server-initiated messages.
    pub fn start_listening(&self) {
        if let Transport::Http(t) = self {
            t.start_get_stream();
        }
    }

    pub async fn close(&self) {
        match self {
            Transport::Stdio(t) => t.close().await,
            Transport::Http(t) => t.close().await,
        }
    }
}
