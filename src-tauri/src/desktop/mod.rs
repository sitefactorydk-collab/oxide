//! Desktop integration (Phase 9): the tray icon, the quick entry window and its global
//! shortcut, deep links, start at login, notifications, window behaviour and the macOS menu.
//!
//! Pure parts (deep-link parsing, shortcut names, notification text) live in
//! `oxide_core::desktop`; this module wires them to Tauri.

pub mod menu;
pub mod notify;
pub mod quick;
pub mod shortcut;
pub mod tray;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError, RwLock};

use oxide_core::{DesktopAction, DesktopSettings};
use tauri::{AppHandle, EventTarget, Manager, WebviewWindow, Window, WindowEvent};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_specta::Event as _;

use crate::events::DesktopRequest;

/// Label of the main window (tauri.conf.json).
pub const MAIN_LABEL: &str = "main";

/// Argument the login item starts Oxide with: stay in the tray, don't show the window.
pub const HIDDEN_ARG: &str = "--hidden";

/// Desktop state the window and tray handlers read synchronously.
#[derive(Default)]
pub struct DesktopState {
    prefs: RwLock<DesktopSettings>,
    shortcut: Mutex<shortcut::ShortcutStatus>,
    autostart_error: Mutex<Option<String>>,
    /// The main window has asked for queued actions, so new ones can be sent as events.
    ready: AtomicBool,
    /// Actions that arrived before the main window was listening (e.g. the deep link Oxide
    /// was started with).
    pending: Mutex<Vec<DesktopAction>>,
    quick: quick::QuickState,
    /// The user has been told once that closing the window keeps Oxide in the tray.
    told_about_tray: AtomicBool,
}

/// Locks a mutex, recovering the data if a thread panicked while holding it (the data here
/// is simple status that stays valid).
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl DesktopState {
    pub fn prefs(&self) -> DesktopSettings {
        self.prefs
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn set_prefs(&self, prefs: DesktopSettings) {
        *self.prefs.write().unwrap_or_else(PoisonError::into_inner) = prefs;
    }

    pub fn shortcut_status(&self) -> shortcut::ShortcutStatus {
        lock(&self.shortcut).clone()
    }

    pub fn autostart_error(&self) -> Option<String> {
        lock(&self.autostart_error).clone()
    }

    /// Marks the main window as listening and returns what arrived before it was.
    pub fn take_pending(&self) -> Vec<DesktopAction> {
        self.ready.store(true, Ordering::SeqCst);
        std::mem::take(&mut *lock(&self.pending))
    }
}

/// Applies desktop settings: the global shortcut and start at login. Called at startup and
/// whenever settings are saved.
pub fn apply_settings(app: &AppHandle, settings: &DesktopSettings) {
    let Some(state) = app.try_state::<DesktopState>() else {
        return;
    };
    state.set_prefs(settings.clone());
    shortcut::apply(app, &state, settings);
    sync_autostart(app, &state, settings.launch_at_login);
}

fn sync_autostart(app: &AppHandle, state: &DesktopState, want: bool) {
    let launcher = app.autolaunch();
    let result = launcher.is_enabled().and_then(|on| {
        if on == want {
            Ok(())
        } else if want {
            launcher.enable()
        } else {
            launcher.disable()
        }
    });
    *lock(&state.autostart_error) = match result {
        Ok(()) => None,
        Err(e) => {
            tracing::warn!("couldn't change start at login: {e}");
            Some(format!("Oxide couldn't change this setting: {e}"))
        }
    };
}

/// Whether start at login is actually on, as the OS reports it.
pub fn autostart_enabled(app: &AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

fn main_window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(MAIN_LABEL)
}

/// Shows, restores and focuses the main window.
pub fn show_main(app: &AppHandle) {
    let Some(w) = main_window(app) else { return };
    if w.is_minimized().unwrap_or(false)
        && let Err(e) = w.unminimize()
    {
        tracing::debug!("unminimize: {e}");
    }
    if let Err(e) = w.show() {
        tracing::warn!("couldn't show the window: {e}");
    }
    if let Err(e) = w.set_focus() {
        tracing::debug!("focus: {e}");
    }
}

/// Tray click: hides the main window when it's in front, shows it otherwise.
pub fn toggle_main(app: &AppHandle) {
    if main_in_front(app) {
        if let Some(w) = main_window(app)
            && let Err(e) = w.hide()
        {
            tracing::warn!("couldn't hide the window: {e}");
        }
    } else {
        show_main(app);
    }
}

/// The main window is visible, not minimized and focused.
pub fn main_in_front(app: &AppHandle) -> bool {
    main_window(app).is_some_and(|w| {
        w.is_visible().unwrap_or(false)
            && !w.is_minimized().unwrap_or(false)
            && w.is_focused().unwrap_or(false)
    })
}

/// Shows the main window and asks it to do `action` (queued until it's listening).
pub fn dispatch(app: &AppHandle, action: DesktopAction) {
    show_main(app);
    let Some(state) = app.try_state::<DesktopState>() else {
        return;
    };
    if !state.ready.load(Ordering::SeqCst) {
        lock(&state.pending).push(action);
        return;
    }
    if let Err(e) = DesktopRequest(action).emit_to(app, EventTarget::webview_window(MAIN_LABEL)) {
        tracing::warn!("couldn't send a desktop action: {e}");
    }
}

/// Handles a deep link (`oxide://…`); unknown links only bring the window forward.
pub fn open_url(app: &AppHandle, url: &str) {
    match oxide_core::desktop::parse_deep_link(url) {
        Some(action) => dispatch(app, action),
        None => {
            tracing::info!("ignored a link Oxide doesn't know");
            show_main(app);
        }
    }
}

/// Saves window positions, then quits.
pub fn quit(app: &AppHandle) {
    use tauri_plugin_window_state::AppHandleExt as _;
    if let Err(e) = app.save_window_state(window_state_flags()) {
        tracing::debug!("window state: {e}");
    }
    app.exit(0);
}

/// What the window-state plugin remembers: size, position, maximized and full screen.
/// Visibility is left out (Oxide decides whether to show the window at startup) and so are
/// decorations (fixed per platform).
pub fn window_state_flags() -> tauri_plugin_window_state::StateFlags {
    use tauri_plugin_window_state::StateFlags;
    StateFlags::SIZE | StateFlags::POSITION | StateFlags::MAXIMIZED | StateFlags::FULLSCREEN
}

/// Window events: the main window's close button hides to the tray (or quits), and the
/// quick entry window hides when it loses focus.
pub fn on_window_event(window: &Window, event: &WindowEvent) {
    let app = window.app_handle();
    match (window.label(), event) {
        (MAIN_LABEL, WindowEvent::CloseRequested { api, .. }) => {
            let Some(state) = app.try_state::<DesktopState>() else {
                return;
            };
            if state.prefs().close_to_tray {
                api.prevent_close();
                if let Err(e) = window.hide() {
                    tracing::warn!("couldn't hide the window: {e}");
                }
                if !state.told_about_tray.swap(true, Ordering::SeqCst) {
                    notify::tray_hint(app);
                }
            } else {
                // Quick entry is a hidden window too; without this the app would keep running.
                api.prevent_close();
                quit(app);
            }
        }
        (quick::QUICK_LABEL, WindowEvent::CloseRequested { api, .. }) => {
            api.prevent_close();
            quick::hide(app);
        }
        (quick::QUICK_LABEL, WindowEvent::Focused(false)) => quick::on_blur(app),
        _ => {}
    }
}
