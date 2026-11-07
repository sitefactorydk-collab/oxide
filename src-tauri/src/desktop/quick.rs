//! The quick entry window: a small frameless composer that floats above other apps.
//! It's created hidden at startup (creating webviews from a shortcut handler can deadlock
//! on Windows) and shown and hidden from then on.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use super::{DesktopState, lock};

/// Label of the quick entry window. The frontend renders it from `index.html?window=quick`.
pub const QUICK_LABEL: &str = "quick";

/// Logical size of the window.
const WIDTH: f64 = 680.0;
const HEIGHT: f64 = 148.0;

/// A blur this soon after showing is ignored: showing a window can briefly hand focus
/// around on some systems.
const BLUR_GRACE: Duration = Duration::from_millis(400);

#[derive(Default)]
pub struct QuickState {
    shown_at: Mutex<Option<Instant>>,
    /// A screenshot is being taken: the window is hidden on purpose and comes back after.
    capturing: Mutex<bool>,
}

/// Creates the (hidden) quick entry window.
pub fn create(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    if let Some(w) = app.get_webview_window(QUICK_LABEL) {
        return Ok(w);
    }
    WebviewWindowBuilder::new(
        app,
        QUICK_LABEL,
        WebviewUrl::App("index.html?window=quick".into()),
    )
    .title("Oxide quick entry")
    .inner_size(WIDTH, HEIGHT)
    .resizable(false)
    .maximizable(false)
    .minimizable(false)
    .decorations(false)
    .shadow(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .visible(false)
    .center()
    .build()
}

fn window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(QUICK_LABEL)
        .or_else(|| match create(app) {
            Ok(w) => Some(w),
            Err(e) => {
                tracing::warn!("couldn't create the quick entry window: {e}");
                None
            }
        })
}

/// Shows quick entry centred on the screen it's on, in front and focused.
pub fn show(app: &AppHandle) {
    let Some(w) = window(app) else { return };
    if let Err(e) = w.center() {
        tracing::debug!("center: {e}");
    }
    reveal(app, &w);
}

fn reveal(app: &AppHandle, w: &WebviewWindow) {
    if let Some(state) = app.try_state::<DesktopState>() {
        *lock(&state.quick.shown_at) = Some(Instant::now());
    }
    if let Err(e) = w.show() {
        tracing::warn!("couldn't show quick entry: {e}");
    }
    if let Err(e) = w.set_focus() {
        tracing::debug!("focus: {e}");
    }
}

pub fn hide(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(QUICK_LABEL)
        && let Err(e) = w.hide()
    {
        tracing::warn!("couldn't hide quick entry: {e}");
    }
}

/// The global shortcut: shows quick entry, or hides it when it's already in front.
pub fn toggle(app: &AppHandle) {
    let in_front = app
        .get_webview_window(QUICK_LABEL)
        .is_some_and(|w| w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false));
    if in_front {
        hide(app);
    } else {
        show(app);
    }
}

/// Clicking anywhere else dismisses quick entry, like a popup.
pub fn on_blur(app: &AppHandle) {
    let Some(state) = app.try_state::<DesktopState>() else {
        return;
    };
    if *lock(&state.quick.capturing) {
        return;
    }
    let recent = lock(&state.quick.shown_at).is_some_and(|t| t.elapsed() < BLUR_GRACE);
    if !recent {
        hide(app);
    }
}

/// Where quick entry is, as a point in physical pixels (its centre), to capture that screen.
pub fn centre(app: &AppHandle) -> Option<(i32, i32)> {
    let w = app.get_webview_window(QUICK_LABEL)?;
    let pos = w.outer_position().ok()?;
    let size = w.outer_size().ok()?;
    let half = |n: u32| i32::try_from(n / 2).unwrap_or(0);
    Some((pos.x + half(size.width), pos.y + half(size.height)))
}

/// Hides quick entry for a screenshot; [`end_capture`] brings it back.
pub fn begin_capture(app: &AppHandle) {
    if let Some(state) = app.try_state::<DesktopState>() {
        *lock(&state.quick.capturing) = true;
    }
    hide(app);
}

pub fn end_capture(app: &AppHandle) {
    if let Some(state) = app.try_state::<DesktopState>() {
        *lock(&state.quick.capturing) = false;
    }
    if let Some(w) = app.get_webview_window(QUICK_LABEL) {
        reveal(app, &w);
    }
}
