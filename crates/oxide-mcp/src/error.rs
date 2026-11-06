use crate::jsonrpc::RpcError;

/// Errors from talking to an MCP server. Messages are written for users and never contain
/// secrets (header values, tokens, env values).
#[derive(Debug, Clone, thiserror::Error)]
pub enum McpError {
    /// The configuration can't work as written.
    #[error("{0}")]
    Config(String),
    /// The process couldn't be started.
    #[error("{0}")]
    Spawn(String),
    /// The process exited or the connection closed.
    #[error("the server stopped{0}")]
    Closed(String),
    #[error("the server didn't answer within {0} s")]
    Timeout(u64),
    /// The server answered with a JSON-RPC error.
    #[error("the server returned an error: {0}")]
    Rpc(RpcError),
    /// An HTTP error status, with the JSON-RPC error in the body if there was one.
    #[error("HTTP {status}{}", detail_suffix(.detail))]
    Http {
        status: u16,
        detail: String,
        rpc: Option<RpcError>,
    },
    /// The server needs an OAuth sign-in (HTTP 401, or 403 asking for more scopes).
    #[error("sign-in required")]
    Unauthorized {
        /// The `WWW-Authenticate` header, if any.
        challenge: Option<String>,
    },
    /// A legacy HTTP session ended (404 on a session id); reconnecting starts a new one.
    #[error("the server ended the session")]
    SessionExpired,
    #[error("couldn't reach the server: {0}")]
    Network(String),
    /// The server broke the protocol.
    #[error("unexpected answer from the server: {0}")]
    Protocol(String),
    #[error("{0}")]
    Auth(String),
    #[error("cancelled")]
    Cancelled,
    /// The server doesn't speak a version Oxide knows.
    #[error("{0}")]
    Unsupported(String),
}

fn detail_suffix(detail: &str) -> String {
    if detail.is_empty() {
        String::new()
    } else {
        format!(": {detail}")
    }
}

impl From<RpcError> for McpError {
    fn from(e: RpcError) -> Self {
        McpError::Rpc(e)
    }
}

impl From<reqwest::Error> for McpError {
    fn from(e: reqwest::Error) -> Self {
        // reqwest errors may include the URL (never headers); strip query strings anyway.
        let mut msg = e.to_string();
        if let Some(q) = msg.find('?') {
            msg.truncate(q);
        }
        if e.is_timeout() {
            return McpError::Network("the request timed out".into());
        }
        McpError::Network(msg)
    }
}

impl McpError {
    /// The JSON-RPC error, if the server sent one (directly or in an HTTP error body).
    pub fn rpc(&self) -> Option<&RpcError> {
        match self {
            McpError::Rpc(e) => Some(e),
            McpError::Http { rpc: Some(e), .. } => Some(e),
            _ => None,
        }
    }
}
