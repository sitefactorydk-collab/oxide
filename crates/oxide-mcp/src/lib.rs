//! Model Context Protocol client: stdio and Streamable HTTP transports, dual-era protocol,
//! server manager, `oxide_config.json`, and OAuth for remote servers.
//!
//! - [`McpClient`] talks to one server. It speaks the stateless 2026-07-28 protocol and falls
//!   back to the `initialize` handshake (2025-11-25 … 2024-11-05) for older servers.
//! - [`McpManager`] runs every configured server: starts, restarts with backoff, stops on
//!   quit, keeps their tool/resource/prompt lists and logs, and reports changes.
//! - [`config`] reads and writes `oxide_config.json` (Claude desktop's `mcpServers` schema)
//!   and imports Claude desktop's config.
//! - [`extensions`] installs desktop extensions (`.mcpb`) and runs their servers through the
//!   manager; [`archive`] reads their zip files.
//! - [`oauth`] signs in to remote servers (OAuth 2.1 + PKCE through the system browser).
//!
//! No Tauri here: the app crate implements [`ManagerSink`] and [`UserAgent`].

pub mod archive;
pub mod client;
pub mod config;
mod error;
pub mod extensions;
pub mod handler;
pub mod jsonrpc;
pub mod log;
pub mod manager;
pub mod oauth;
pub mod process;
pub mod protocol;
mod sse;
pub mod transport;
pub mod trust;

pub use client::{ConnectOptions, McpClient, ServerInfo};
pub use error::McpError;
pub use extensions::Extensions;
pub use handler::{ClientEvent, ClientHandler, NoUser};
pub use manager::{ExtensionServer, ManagerOptions, ManagerSink, McpManager, UserAgent};
