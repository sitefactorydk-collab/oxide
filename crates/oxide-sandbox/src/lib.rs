//! Oxide's Docker sandbox (Phase 12b): an isolated workspace per chat.
//!
//! Each chat that uses the sandbox gets a container (`oxide-sandbox-<chat id>`) from the
//! configured image, with its workspace — a Docker volume or a folder Oxide made for it —
//! mounted at `/workspace` and nothing else from this computer. Containers run without
//! extra privileges, with CPU, memory and process limits and (by default) no network, and
//! stop when idle. Models run commands there ([`Sandboxes::exec`]), use the reference
//! filesystem MCP server's tools on the workspace ([`Sandboxes::call_file_tool`]), and move
//! files in and out only through [`Sandboxes::copy_in`] and [`Sandboxes::copy_out`], which
//! the chat service asks the user about.
//!
//! Everything goes through the `docker` command line ([`Docker`]). No Tauri here.

mod docker;
mod error;
mod files;
mod manager;
mod script;

pub use docker::{Docker, MAX_CAPTURE_BYTES, Output};
pub use error::SandboxError;
pub use manager::{ExecResult, MAX_COPY_BYTES, NoSink, SandboxSink, Sandboxes};
