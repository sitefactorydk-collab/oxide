//! Errors from reply generation. Messages can be shown to the user as they are.

use oxide_llm::LlmError;
use oxide_store::StoreError;

#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Llm(#[from] LlmError),
    #[error("a reply is already being written in this chat")]
    Busy,
    #[error("{0} not found")]
    NotFound(&'static str),
    #[error("{0}")]
    Invalid(String),
}
