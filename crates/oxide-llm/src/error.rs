//! Errors from the model APIs. Messages are written to be shown to the user as they are and
//! never contain API keys or request headers.

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("couldn't reach {url}: {reason}")]
    Unreachable { url: String, reason: String },
    #[error("the API key was rejected ({status})")]
    Unauthorized { status: u16 },
    #[error("no API key is set for this endpoint. Add one in Settings → Providers.")]
    MissingKey,
    #[error("the provider is rate limiting requests: {message}")]
    RateLimited { message: String },
    #[error("the provider is overloaded right now. Try again in a moment.")]
    Overloaded,
    #[error("the server answered {status}: {message}")]
    Http { status: u16, message: String },
    #[error("unexpected response: {0}")]
    Decode(String),
    #[error("invalid request: {0}")]
    Invalid(String),
    /// The reply broke off part-way (a stream error event or a dropped connection).
    #[error("{0}")]
    Stream(String),
}

impl LlmError {
    /// Whether waiting and sending the same request again may succeed.
    pub fn is_retryable(&self) -> bool {
        matches!(self, LlmError::RateLimited { .. } | LlmError::Overloaded)
    }
}
