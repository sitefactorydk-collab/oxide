//! Reply generation for Oxide, and (from Phase 6) the agent loop, tool registry, approvals
//! and built-in tools.
//!
//! - [`ModelRegistry`]: the models every enabled endpoint offers, with capabilities.
//! - [`ChatService`]: writes replies. It builds the context from the visible thread, streams
//!   the model's answer to an [`EventSink`], saves the reply (also when stopped or failed
//!   part-way) and names new chats after their first exchange.
//!
//! - [`AttachmentStore`]: checks, converts and stores attached files.
//! - [`export`]: conversations as Markdown or JSON.
//! - [`knowledge`]: project knowledge files, full or searched (`project_search`).
//! - [`artifacts`]: the `artifact` tool (documents, code, pages, diagrams, React).
//! - [`memory`]: learning memory items from chats (opt-in).
//!
//! This crate has no Tauri dependency: the app crate implements [`EventSink`].

pub mod artifacts;
pub mod attachments;
mod chat;
mod context;
pub mod demo;
mod error;
pub mod export;
pub mod knowledge;
pub mod memory;
mod registry;
pub mod sampling;
mod title;
pub mod tools;

pub use attachments::{AttachmentError, AttachmentStore};
pub use chat::{ChatService, EventSink, path_to};
pub use context::{
    PromptContext, build_messages, plan_request, reply_turns, request_messages, system_prompt,
};
pub use error::AgentError;
pub use registry::{Connector, ModelRegistry};
pub use sampling::run_sampling;
pub use title::{clean_title, pick_title_model};
pub use tools::{McpTools, Tools, WebTools};
