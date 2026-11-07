//! System notifications: a reply finished, failed or needs a tool approval while Oxide
//! isn't in front.

use oxide_core::{ChatStreamEvent, StreamEvent};
use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt as _;

use super::{DesktopState, main_in_front};
use crate::state::AppState;

/// Called for every reply event; notifies for the ones that matter when the main window
/// isn't in front and notifications are on.
pub fn on_stream_event(app: &AppHandle, event: &ChatStreamEvent) {
    let body = match &event.event {
        StreamEvent::Done { message: Some(m) } => oxide_core::desktop::reply_preview(&m.content),
        StreamEvent::Failed { .. } => "The reply stopped with an error.".to_owned(),
        StreamEvent::ToolApproval { request } => format!(
            "Waiting for your approval to use {}.",
            request.title.as_deref().unwrap_or(&request.tool)
        ),
        _ => return,
    };
    let Some(state) = app.try_state::<DesktopState>() else {
        return;
    };
    if !state.prefs().notifications || main_in_front(app) {
        return;
    }
    let app = app.clone();
    let id = event.conversation_id.clone();
    tauri::async_runtime::spawn(async move {
        let title = match app.try_state::<AppState>() {
            Some(s) => s
                .store
                .get_conversation(&id)
                .await
                .ok()
                .map(|c| c.title)
                .filter(|t| !t.trim().is_empty()),
            None => None,
        };
        show(&app, title.as_deref().unwrap_or("Oxide"), &body);
    });
}

/// Once per run, the first time the close button hides Oxide: say where it went.
pub fn tray_hint(app: &AppHandle) {
    let Some(state) = app.try_state::<DesktopState>() else {
        return;
    };
    if state.prefs().notifications {
        show(
            app,
            "Oxide is still running",
            "It's in the tray. Use the tray icon to open it again or to quit.",
        );
    }
}

fn show(app: &AppHandle, title: &str, body: &str) {
    if let Err(e) = app.notification().builder().title(title).body(body).show() {
        tracing::warn!("couldn't show a notification: {e}");
    }
}
