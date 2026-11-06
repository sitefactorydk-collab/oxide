//! What a server can ask of Oxide, and what it tells Oxide on its own.

use futures_util::future::BoxFuture;
use serde_json::{Value, json};

use crate::jsonrpc::RpcError;

/// Something a server reported outside a response.
#[derive(Debug, Clone, PartialEq)]
pub enum ClientEvent {
    /// `notifications/message`.
    Log {
        level: String,
        text: String,
    },
    /// `notifications/progress` for a request that carried `token`.
    Progress {
        token: String,
        progress: f64,
        total: Option<f64>,
        message: Option<String>,
    },
    /// Tools, resources or prompts changed.
    ListChanged,
    ResourceUpdated {
        uri: String,
    },
}

/// Answers server-to-client requests (legacy server requests and modern MRTR input
/// requests alike) and receives events. Implemented by the server manager.
pub trait ClientHandler: Send + Sync + 'static {
    /// `sampling/createMessage`: the result object, or an error if the user refused.
    fn sample<'a>(
        &'a self,
        server: &'a str,
        params: Value,
    ) -> BoxFuture<'a, Result<Value, RpcError>>;
    /// `elicitation/create`: an `ElicitResult` (`action` accept / decline / cancel).
    fn elicit<'a>(
        &'a self,
        server: &'a str,
        params: Value,
    ) -> BoxFuture<'a, Result<Value, RpcError>>;
    /// `roots/list`: `Root` objects (`uri`, `name`).
    fn roots<'a>(&'a self, server: &'a str) -> BoxFuture<'a, Vec<Value>>;
    fn event(&self, server: &str, event: ClientEvent);
}

/// Refuses sampling, declines elicitation, has no roots, ignores events. For tests and
/// one-off connections.
pub struct NoUser;

impl ClientHandler for NoUser {
    fn sample<'a>(
        &'a self,
        _server: &'a str,
        _params: Value,
    ) -> BoxFuture<'a, Result<Value, RpcError>> {
        Box::pin(async { Err(RpcError::new(-1, "sampling isn't available")) })
    }

    fn elicit<'a>(
        &'a self,
        _server: &'a str,
        _params: Value,
    ) -> BoxFuture<'a, Result<Value, RpcError>> {
        Box::pin(async { Ok(json!({"action": "decline"})) })
    }

    fn roots<'a>(&'a self, _server: &'a str) -> BoxFuture<'a, Vec<Value>> {
        Box::pin(async { Vec::new() })
    }

    fn event(&self, _server: &str, _event: ClientEvent) {}
}
