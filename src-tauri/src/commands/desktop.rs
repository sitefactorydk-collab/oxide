//! Desktop integration commands: quick entry, the main window's queued actions, and the
//! state Settings → Desktop shows.

use base64::Engine as _;
use oxide_core::{Attachment, AttachmentSource, DesktopAction};
use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, State};

use crate::desktop::{self, DesktopState, quick, shortcut::ShortcutStatus};
use crate::error::{CmdResult, CommandError};
use crate::state::AppState;

/// What Settings → Desktop shows besides the saved settings.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DesktopStatus {
    pub shortcut: ShortcutStatus,
    /// Start at login as the operating system reports it.
    pub launch_at_login: bool,
    pub launch_at_login_error: Option<String>,
    /// The deep-link scheme (`oxide`).
    pub deep_link_scheme: String,
    /// A development build: start at login and deep links point at the dev binary.
    pub dev_build: bool,
}

#[tauri::command]
#[specta::specta]
pub fn desktop_status(app: AppHandle, state: State<'_, DesktopState>) -> DesktopStatus {
    DesktopStatus {
        shortcut: state.shortcut_status(),
        launch_at_login: desktop::autostart_enabled(&app),
        launch_at_login_error: state.autostart_error(),
        deep_link_scheme: oxide_core::desktop::DEEP_LINK_SCHEME.into(),
        dev_build: cfg!(debug_assertions),
    }
}

/// The main window is listening: returns actions that arrived before (e.g. the deep link
/// Oxide was started with). Later ones come as `desktopRequest` events.
#[tauri::command]
#[specta::specta]
pub fn take_desktop_actions(state: State<'_, DesktopState>) -> Vec<DesktopAction> {
    state.take_pending()
}

#[tauri::command]
#[specta::specta]
pub fn show_quick_entry(app: AppHandle) {
    quick::show(&app);
}

#[tauri::command]
#[specta::specta]
pub fn hide_quick_entry(app: AppHandle) {
    quick::hide(&app);
}

/// Quick entry started a chat: hide it and open the chat in the main window.
#[tauri::command]
#[specta::specta]
pub fn open_quick_entry_chat(app: AppHandle, conversation_id: String) {
    quick::hide(&app);
    desktop::dispatch(&app, DesktopAction::OpenChat { conversation_id });
}

/// Takes a screenshot of the screen quick entry is on (with quick entry hidden) and prepares
/// it as an attachment.
#[tauri::command]
#[specta::specta]
pub async fn capture_screenshot(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<Attachment> {
    let point = quick::centre(&app);
    quick::begin_capture(&app);
    // Give the compositor time to remove the window from the screen.
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    let shot = tokio::task::spawn_blocking(move || oxide_computer::capture_screen_png(point)).await;
    quick::end_capture(&app);
    let png = shot
        .map_err(|_| CommandError::Internal("The screenshot couldn't be taken.".into()))?
        .map_err(|e| {
            tracing::warn!("screenshot: {e}");
            CommandError::Invalid(format!("The screenshot couldn't be taken: {e}"))
        })?;
    let data = base64::engine::general_purpose::STANDARD.encode(&png);
    Ok(state
        .attachments
        .prepare(AttachmentSource::Bytes {
            name: "Screenshot.png".into(),
            data,
        })
        .await?)
}
