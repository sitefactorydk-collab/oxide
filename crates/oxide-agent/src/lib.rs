//! Reply generation for Oxide, and (from Phase 6) the agent loop, tool registry, approvals
//! and built-in tools.
//!
//! - [`ModelRegistry`]: the models every enabled endpoint offers, with capabilities.
//! - [`ChatService`]: writes replies. It builds the context from the visible thread, streams
//!   the model's answer to an [`EventSink`], saves the reply (also when stopped or failed
//!   part-way) and names new chats after their first exchange.
//!
//! This crate has no Tauri dependency: the app crate implements [`EventSink`].

mod chat;
mod context;
mod error;
mod registry;
mod title;

pub use chat::{ChatService, EventSink};
pub use context::{build_messages, plan_request, system_prompt};
pub use error::AgentError;
pub use registry::{Connector, ModelRegistry};
pub use title::{clean_title, pick_title_model};
