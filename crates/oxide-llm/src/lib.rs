//! LLM providers for Oxide.
//!
//! - [`Provider`]: lists an endpoint's models and streams replies as [`ChatEvent`]s.
//! - [`anthropic`]: the Messages API (SSE, extended thinking, tools, images, PDFs, prompt
//!   caching, retry on 429/529).
//! - [`openai`]: OpenAI-compatible Chat Completions, used for OpenAI, LM Studio and Strata.
//! - [`capabilities`]: which models support tools, thinking, images and PDFs.
//! - [`probe`]: the connection test behind Settings → Providers.
//!
//! Both clients are hand-written on `reqwest` and share one SSE parser ([`sse`]). API keys
//! never appear in errors, logs or `Debug` output.

pub mod anthropic;
pub mod capabilities;
mod error;
pub mod generation;
mod http;
pub mod media;
pub mod openai;
pub mod probe;
mod provider;
pub mod sse;
mod stream;
pub mod transcribe;
mod types;

#[cfg(test)]
pub(crate) mod test_server;

pub use error::{ContextNumbers, LlmError, context_numbers};
pub use provider::{Endpoint, ModelListing, Provider, complete_text, connect};
pub use types::{
    ChatEvent, ChatMessage, ChatRequest, ChatStream, ComputerFlavor, ComputerTool, Thinking,
    ToolResultClearing, ToolSpec,
};
