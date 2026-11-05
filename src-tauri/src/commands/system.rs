//! Links and other hand-offs to the operating system.

use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

use crate::error::{CmdResult, CommandError};

/// Opens a web or mail link in the default browser or mail app. Other schemes are refused.
#[tauri::command]
#[specta::specta]
pub fn open_external(app: AppHandle, url: String) -> CmdResult<()> {
    let url = url.trim();
    let lower = url.to_ascii_lowercase();
    if !(lower.starts_with("https://")
        || lower.starts_with("http://")
        || lower.starts_with("mailto:"))
    {
        return Err(CommandError::Invalid(
            "only web and mail links can be opened".into(),
        ));
    }
    app.opener().open_url(url, None::<&str>).map_err(|e| {
        tracing::warn!("couldn't open a link: {e}");
        CommandError::Internal("The link couldn't be opened.".into())
    })
}
