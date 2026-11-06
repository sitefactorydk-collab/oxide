//! Tauri commands exposed to the frontend. Each one is thin glue over `oxide-store` /
//! `oxide-llm`; register new ones in `specta_builder` (lib.rs) and regenerate the bindings.

pub mod attachments;
pub mod chats;
pub mod mcp;
pub mod models;
pub mod projects;
pub mod providers;
pub mod settings;
pub mod system;
pub mod tools;

use serde::Serialize;
use specta::Type;

/// Response of [`ping`]: proves the IPC round trip and the generated bindings work.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PingResponse {
    pub message: String,
    pub app_name: String,
    pub app_version: String,
    pub core_version: String,
}

/// Health check used by the frontend on startup.
#[tauri::command]
#[specta::specta]
pub fn ping(name: String) -> PingResponse {
    PingResponse {
        message: format!("pong, {name}"),
        app_name: oxide_core::APP_NAME.to_owned(),
        app_version: env!("CARGO_PKG_VERSION").to_owned(),
        core_version: oxide_core::version().to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_echoes_name() {
        let res = ping("Oxide".into());
        assert_eq!(res.message, "pong, Oxide");
        assert_eq!(res.app_name, "Oxide");
    }
}
