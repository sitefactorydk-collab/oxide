//! System notifications: a reply finished, failed or needs a tool approval while Oxide
//! isn't in front.

use oxide_core::{ChatStreamEvent, RunStatus, ScheduledTask, StreamEvent, TaskRun};
use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt as _;

use super::{DesktopState, main_in_front};
use crate::state::AppState;

/// Called for every reply event; notifies for the ones that matter when the main window
/// isn't in front and notifications are on.
pub fn on_stream_event(app: &AppHandle, event: &ChatStreamEvent) {
    // A scheduled run's chat: the scheduler sends its own notifications (`on_task_run`).
    if app
        .try_state::<AppState>()
        .is_some_and(|s| s.chat.is_unattended(&event.conversation_id))
    {
        return;
    }
    let body = match &event.event {
        StreamEvent::Done { message: Some(m) } => oxide_core::desktop::reply_preview(&m.content),
        StreamEvent::Failed { .. } => "The reply stopped with an error.".to_owned(),
        StreamEvent::ToolApproval { request } => format!(
            "Waiting for your approval to use {}.",
            request.title.as_deref().unwrap_or(&request.tool)
        ),
        _ => return,
    };
    let kind = match &event.event {
        StreamEvent::Done { .. } => "reply finished",
        StreamEvent::Failed { .. } => "reply failed",
        _ => "approval needed",
    };
    let Some(state) = app.try_state::<DesktopState>() else {
        return;
    };
    if !state.prefs().notifications {
        tracing::debug!(kind, "no notification: notifications are off");
        return;
    }
    if main_in_front(app) {
        tracing::debug!(kind, "no notification: Oxide is in front");
        return;
    }
    tracing::debug!(kind, conversation = %event.conversation_id, "notifying");
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

/// A scheduled run needs the user, finished or failed. Notifies while Oxide isn't in front
/// (in front, the window shows it); missed, skipped and stopped runs only go in the history.
pub fn on_task_run(app: &AppHandle, task: &ScheduledTask, run: &TaskRun) {
    let body = match run.status {
        RunStatus::WaitingApproval => {
            "A scheduled run is waiting for your approval to use a tool.".to_owned()
        }
        RunStatus::Done => match run.output.as_deref().map(str::trim) {
            Some(o) if !o.is_empty() => {
                oxide_core::desktop::reply_preview(&[oxide_core::ContentBlock::text(
                    o.trim_start_matches('…'),
                )])
            }
            _ => "The scheduled run finished.".to_owned(),
        },
        RunStatus::Failed => format!(
            "The scheduled run failed: {}",
            run.error.as_deref().unwrap_or("unknown error")
        ),
        _ => return,
    };
    let Some(state) = app.try_state::<DesktopState>() else {
        return;
    };
    if !state.prefs().notifications {
        return;
    }
    if main_in_front(app) {
        tracing::debug!("no notification for a scheduled run: Oxide is in front");
        return;
    }
    tracing::debug!(task = %task.id, status = run.status.as_str(), "notifying about a run");
    show(app, &task.name, &body);
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

/// The main window was just hidden while tool calls wait for approval: say so, since the
/// cards' own notifications went out while Oxide was in front.
pub fn waiting_reminder(app: &AppHandle) {
    let Some(state) = app.try_state::<DesktopState>() else {
        return;
    };
    if !state.prefs().notifications {
        return;
    }
    let Some(app_state) = app.try_state::<AppState>() else {
        return;
    };
    let waiting = app_state.chat.pending_approvals().len();
    if waiting == 0 {
        return;
    }
    tracing::debug!(waiting, "hidden with approvals waiting: notifying");
    let body = if waiting == 1 {
        "A tool call is waiting for your approval.".to_owned()
    } else {
        format!("{waiting} tool calls are waiting for your approval.")
    };
    show(app, "Oxide needs your input", &body);
}

/// Settings → Desktop → “Send a test notification”.
pub fn test(app: &AppHandle) -> Result<(), String> {
    tracing::info!("sending a test notification");
    app.notification()
        .builder()
        .title("Oxide")
        .body("Notifications work. You'll see these when a reply finishes or needs you.")
        .show()
        .map_err(|e| e.to_string())
}

fn show(app: &AppHandle, title: &str, body: &str) {
    match app.notification().builder().title(title).body(body).show() {
        Ok(()) => tracing::debug!("notification shown: {title}"),
        Err(e) => tracing::warn!("couldn't show a notification: {e}"),
    }
}
