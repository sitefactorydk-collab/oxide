//! JSON-RPC 2.0 messages as MCP uses them.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

/// Standard JSON-RPC codes.
pub const PARSE_ERROR: i64 = -32700;
pub const INVALID_REQUEST: i64 = -32600;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;
pub const INTERNAL_ERROR: i64 = -32603;

/// MCP codes (2026-07-28). Errors with these codes come from modern servers only.
pub const HEADER_MISMATCH: i64 = -32020;
pub const MISSING_CLIENT_CAPABILITY: i64 = -32021;
pub const UNSUPPORTED_PROTOCOL_VERSION: i64 = -32022;

/// Legacy MCP codes, still accepted from older servers.
pub const LEGACY_RESOURCE_NOT_FOUND: i64 = -32002;

/// The id of a request. Never `null` in MCP.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RequestId {
    Num(i64),
    Str(String),
}

impl std::fmt::Display for RequestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RequestId::Num(n) => write!(f, "{n}"),
            RequestId::Str(s) => f.write_str(s),
        }
    }
}

impl From<RequestId> for Value {
    fn from(id: RequestId) -> Value {
        match id {
            RequestId::Num(n) => json!(n),
            RequestId::Str(s) => json!(s),
        }
    }
}

/// The `error` member of a response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, thiserror::Error)]
#[error("{message} (code {code})")]
pub struct RpcError {
    pub code: i64,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl RpcError {
    pub fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    /// An error only a 2026-07-28 (or later) server would send.
    pub fn is_modern(&self) -> bool {
        matches!(
            self.code,
            HEADER_MISMATCH | MISSING_CLIENT_CAPABILITY | UNSUPPORTED_PROTOCOL_VERSION
        )
    }

    /// The versions listed by an `UnsupportedProtocolVersionError`.
    pub fn supported_versions(&self) -> Vec<String> {
        if self.code != UNSUPPORTED_PROTOCOL_VERSION {
            return Vec::new();
        }
        self.data
            .as_ref()
            .and_then(|d| d.get("supported"))
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// A message from the server.
#[derive(Debug, Clone, PartialEq)]
pub enum Incoming {
    /// A request to the client (legacy servers only).
    Request {
        id: RequestId,
        method: String,
        params: Value,
    },
    Notification {
        method: String,
        params: Value,
    },
    /// The answer to one of our requests. `id` is `None` only for errors about unreadable
    /// requests.
    Response {
        id: Option<RequestId>,
        outcome: Result<Value, RpcError>,
    },
}

/// Reads one JSON-RPC message. `None` if `value` isn't one.
pub fn parse(value: Value) -> Option<Incoming> {
    let Value::Object(mut obj) = value else {
        return None;
    };
    let id = obj
        .remove("id")
        .and_then(|v| serde_json::from_value::<RequestId>(v).ok());
    if let Some(Value::String(method)) = obj.remove("method") {
        let params = obj.remove("params").unwrap_or(Value::Null);
        return Some(match id {
            Some(id) => Incoming::Request { id, method, params },
            None => Incoming::Notification { method, params },
        });
    }
    if let Some(err) = obj.remove("error") {
        let err = serde_json::from_value::<RpcError>(err)
            .unwrap_or_else(|_| RpcError::new(INTERNAL_ERROR, "malformed error response"));
        return Some(Incoming::Response {
            id,
            outcome: Err(err),
        });
    }
    if let Some(result) = obj.remove("result") {
        return Some(Incoming::Response {
            id,
            outcome: Ok(result),
        });
    }
    None
}

/// Reads a line or body of text. JSON-RPC batches (arrays) are split into their messages.
pub fn parse_text(text: &str) -> Result<Vec<Incoming>, serde_json::Error> {
    let value: Value = serde_json::from_str(text)?;
    Ok(match value {
        Value::Array(items) => items.into_iter().filter_map(parse).collect(),
        other => parse(other).into_iter().collect(),
    })
}

pub fn request(id: &RequestId, method: &str, params: &Value) -> Value {
    let mut m = Map::new();
    m.insert("jsonrpc".into(), json!("2.0"));
    m.insert("id".into(), id.clone().into());
    m.insert("method".into(), json!(method));
    if !params.is_null() {
        m.insert("params".into(), params.clone());
    }
    Value::Object(m)
}

pub fn notification(method: &str, params: &Value) -> Value {
    let mut m = Map::new();
    m.insert("jsonrpc".into(), json!("2.0"));
    m.insert("method".into(), json!(method));
    if !params.is_null() {
        m.insert("params".into(), params.clone());
    }
    Value::Object(m)
}

pub fn response(id: &RequestId, outcome: &Result<Value, RpcError>) -> Value {
    match outcome {
        Ok(result) => json!({"jsonrpc": "2.0", "id": Value::from(id.clone()), "result": result}),
        Err(e) => json!({"jsonrpc": "2.0", "id": Value::from(id.clone()), "error": e}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_kind() {
        let msgs = parse_text(
            r#"[{"jsonrpc":"2.0","id":1,"result":{"x":1}},
                {"jsonrpc":"2.0","id":"a","method":"ping"},
                {"jsonrpc":"2.0","method":"notifications/progress","params":{"progress":1}},
                {"jsonrpc":"2.0","id":2,"error":{"code":-32601,"message":"nope"}}]"#,
        )
        .expect("parses");
        assert_eq!(msgs.len(), 4);
        assert!(matches!(
            &msgs[0],
            Incoming::Response {
                id: Some(RequestId::Num(1)),
                outcome: Ok(_)
            }
        ));
        assert!(
            matches!(&msgs[1], Incoming::Request { id: RequestId::Str(s), method, .. } if s == "a" && method == "ping")
        );
        assert!(
            matches!(&msgs[2], Incoming::Notification { method, .. } if method == "notifications/progress")
        );
        assert!(
            matches!(&msgs[3], Incoming::Response { outcome: Err(e), .. } if e.code == METHOD_NOT_FOUND)
        );
    }

    #[test]
    fn modern_errors_are_recognised() {
        let e: RpcError = serde_json::from_value(json!({
            "code": -32022, "message": "Unsupported protocol version",
            "data": {"supported": ["2026-07-28", "2025-11-25"], "requested": "1900-01-01"}
        }))
        .expect("parses");
        assert!(e.is_modern());
        assert_eq!(e.supported_versions(), vec!["2026-07-28", "2025-11-25"]);
        assert!(!RpcError::new(METHOD_NOT_FOUND, "x").is_modern());
    }

    #[test]
    fn builds_messages() {
        let r = request(&RequestId::Num(3), "tools/list", &Value::Null);
        assert_eq!(r, json!({"jsonrpc":"2.0","id":3,"method":"tools/list"}));
        let e = response(
            &RequestId::Str("x".into()),
            &Err(RpcError::new(METHOD_NOT_FOUND, "no")),
        );
        assert_eq!(e["error"]["code"], json!(-32601));
    }
}
