//! The error type every fallible command returns. Serialized to the frontend as
//! `{ kind, message }`; messages never contain API keys or message content.

use oxide_agent::AgentError;
use oxide_store::{SecretError, StoreError};
use serde::Serialize;
use specta::Type;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", content = "message", rename_all = "camelCase")]
pub enum CommandError {
    /// The thing asked for doesn't exist (any more).
    NotFound(String),
    /// The request was rejected; the message says why and can be shown as is.
    Invalid(String),
    /// The OS keychain failed or is unavailable.
    Keychain(String),
    /// Anything else (database, I/O). Details are in the log.
    Internal(String),
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommandError::NotFound(m)
            | CommandError::Invalid(m)
            | CommandError::Keychain(m)
            | CommandError::Internal(m) => f.write_str(m),
        }
    }
}

impl From<StoreError> for CommandError {
    fn from(e: StoreError) -> Self {
        match e {
            StoreError::NotFound { what } => CommandError::NotFound(format!("{what} not found")),
            StoreError::Invalid(m) => CommandError::Invalid(m),
            StoreError::Secret(s) => s.into(),
            other => {
                tracing::error!("store error: {other}");
                CommandError::Internal("Something went wrong saving or loading data.".into())
            }
        }
    }
}

impl From<SecretError> for CommandError {
    fn from(e: SecretError) -> Self {
        tracing::warn!("keychain error: {e}");
        CommandError::Keychain(e.to_string())
    }
}

impl From<AgentError> for CommandError {
    fn from(e: AgentError) -> Self {
        match e {
            AgentError::Store(s) => s.into(),
            AgentError::NotFound(what) => CommandError::NotFound(format!("{what} not found")),
            // Provider errors are written for users and never contain keys.
            AgentError::Llm(l) => CommandError::Invalid(l.to_string()),
            AgentError::Busy | AgentError::Invalid(_) | AgentError::Attachment(_) => {
                CommandError::Invalid(e.to_string())
            }
        }
    }
}

impl From<oxide_agent::AttachmentError> for CommandError {
    fn from(e: oxide_agent::AttachmentError) -> Self {
        CommandError::Invalid(e.to_string())
    }
}

impl From<oxide_mcp::McpError> for CommandError {
    fn from(e: oxide_mcp::McpError) -> Self {
        // MCP errors are written for users and never contain secrets.
        let text = e.to_string();
        let mut chars = text.chars();
        let text = match chars.next() {
            Some(f) => f.to_uppercase().chain(chars).collect(),
            None => text,
        };
        CommandError::Invalid(text)
    }
}

pub type CmdResult<T> = Result<T, CommandError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_as_kind_and_message() {
        let json = serde_json::to_value(CommandError::NotFound("chat not found".into()))
            .expect("serializes");
        assert_eq!(
            json,
            serde_json::json!({"kind": "notFound", "message": "chat not found"})
        );
    }

    #[test]
    fn store_errors_map_to_kinds() {
        let e: CommandError = StoreError::NotFound { what: "project" }.into();
        assert_eq!(e, CommandError::NotFound("project not found".into()));
        let e: CommandError = StoreError::Corrupt("x".into()).into();
        assert!(matches!(e, CommandError::Internal(_)));
    }

    #[test]
    fn agent_errors_map_to_kinds() {
        let e: CommandError = AgentError::Busy.into();
        assert_eq!(
            e,
            CommandError::Invalid("a reply is already being written in this chat".into())
        );
        let e: CommandError = AgentError::NotFound("message").into();
        assert_eq!(e, CommandError::NotFound("message not found".into()));
        let e: CommandError = AgentError::Store(StoreError::NotFound {
            what: "conversation",
        })
        .into();
        assert_eq!(e, CommandError::NotFound("conversation not found".into()));
    }
}
