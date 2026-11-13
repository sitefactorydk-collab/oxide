//! Computer use in the app (Phase 16): the real screen and input behind `oxide-agent`'s
//! `ComputerControl`, and the control bar, Oxide's window and the panic shortcut behind its
//! `ComputerSink`.
//!
//! The control bar is a small always-on-top window (`index.html?window=computer`) at the top
//! of the screen models use. It can't take focus (clicking Stop leaves the app being used in
//! front) and is left out of screenshots. It's created hidden at startup, like quick entry
//! (creating webviews from background threads can deadlock on Windows).

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};

use oxide_agent::{ComputerControl, ComputerSink, Press};
use oxide_computer::Desktop;
use oxide_core::computer::{
    AppIdentity, ComputerActivity, ComputerActivityState, ComputerDisplay, FieldInfo, KeyName,
    MouseButton,
};
use tauri::{
    AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};
use tauri_specta::Event as _;

use crate::events::ComputerChanged;

/// Label of the control bar window.
pub const BAR_LABEL: &str = "computer";

/// Logical size of the bar.
const BAR_WIDTH: f64 = 520.0;
const BAR_HEIGHT: f64 = 44.0;
/// Logical gap between the top of the screen and the bar.
const BAR_TOP: f64 = 6.0;

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

// ---- The real computer -----------------------------------------------------------------------

/// `oxide-computer`'s desktop as the agent's `ComputerControl`.
pub struct DesktopControl(pub Desktop);

fn press(p: Press) -> oxide_computer::Press {
    match p {
        Press::Down => oxide_computer::Press::Down,
        Press::Up => oxide_computer::Press::Up,
        Press::Click => oxide_computer::Press::Click,
    }
}

impl ComputerControl for DesktopControl {
    fn displays(&self) -> Result<Vec<ComputerDisplay>, String> {
        self.0.displays()
    }
    fn capture(&self, display: &ComputerDisplay) -> Result<image::RgbaImage, String> {
        self.0.capture(display)
    }
    fn move_to(&self, x: i32, y: i32) -> Result<(), String> {
        self.0.move_to(x, y)
    }
    fn button(&self, button: MouseButton, p: Press) -> Result<(), String> {
        self.0.button(button, press(p))
    }
    fn key(&self, key: KeyName, p: Press) -> Result<(), String> {
        self.0.key(key, press(p))
    }
    fn text(&self, text: &str) -> Result<(), String> {
        self.0.text(text)
    }
    fn scroll(&self, dx: i32, dy: i32) -> Result<(), String> {
        self.0.scroll(dx, dy)
    }
    fn cursor(&self) -> Result<(i32, i32), String> {
        self.0.cursor()
    }
    fn app_at(&self, x: i32, y: i32) -> Option<AppIdentity> {
        self.0.app_at(x, y)
    }
    fn foreground_app(&self) -> Option<AppIdentity> {
        self.0.foreground_app()
    }
    fn focused_field(&self) -> Option<FieldInfo> {
        self.0.focused_field()
    }
}

// ---- The bar, Oxide's window and the panic shortcut --------------------------------------------

/// The app's side of computer use.
pub struct TauriComputerSink {
    app: AppHandle,
    /// Chats acting or waiting, by id.
    chats: Mutex<BTreeMap<String, ComputerActivity>>,
    /// Oxide's main window was minimized for computer use (and comes back after).
    hid_main: AtomicBool,
    /// The screen in Settings → Computer use (`None`: the main one), for placing the bar.
    display: Mutex<Option<String>>,
}

impl TauriComputerSink {
    pub fn new(app: AppHandle) -> Self {
        Self {
            app,
            chats: Mutex::default(),
            hid_main: AtomicBool::new(false),
            display: Mutex::default(),
        }
    }

    /// Settings changed: the screen the bar goes on.
    pub fn set_display(&self, display: Option<String>) {
        *lock(&self.display) = display;
    }

    /// The user's home, Desktop, Documents and Downloads folders (Windows' known folders,
    /// OneDrive moves included), told to models so they don't guess paths.
    pub fn known_folders(&self) -> Vec<(String, String)> {
        let path = self.app.path();
        [
            ("Home", path.home_dir()),
            ("Desktop", path.desktop_dir()),
            ("Documents", path.document_dir()),
            ("Downloads", path.download_dir()),
        ]
        .into_iter()
        .filter_map(|(name, dir)| Some((name.to_owned(), dir.ok()?.display().to_string())))
        .collect()
    }

    /// The chats' activity, for a window that opens later.
    pub fn activity(&self) -> Vec<ComputerActivity> {
        lock(&self.chats).values().cloned().collect()
    }

    fn update(&self, cid: &str, state: ComputerActivityState, action: Option<String>) {
        let changed = ComputerActivity {
            conversation_id: cid.to_owned(),
            state,
            action,
        };
        let chats = {
            let mut chats = lock(&self.chats);
            if state == ComputerActivityState::Ended {
                chats.remove(cid);
            } else {
                chats.insert(cid.to_owned(), changed.clone());
            }
            chats.values().cloned().collect::<Vec<_>>()
        };
        let working = chats
            .iter()
            .any(|c| c.state == ComputerActivityState::Working);
        if chats.is_empty() {
            hide_bar(&self.app);
        } else {
            show_bar(&self.app, lock(&self.display).as_deref());
        }
        // Oxide's window comes back when nothing acts (a card waits, or all ended).
        if !working && self.hid_main.swap(false, Ordering::SeqCst) {
            crate::desktop::show_main(&self.app);
        }
        if let Err(e) = (ComputerChanged { chats, changed }).emit(&self.app) {
            tracing::warn!("couldn't send computer activity: {e}");
        }
    }
}

impl ComputerSink for TauriComputerSink {
    fn working(&self, cid: &str, action: &str, hide: bool) -> bool {
        self.update(cid, ComputerActivityState::Working, Some(action.to_owned()));
        if !hide {
            return false;
        }
        // Minimized, not hidden: it stays in the taskbar, and the user can bring it back.
        let Some(w) = self.app.get_webview_window(crate::desktop::MAIN_LABEL) else {
            return false;
        };
        let shown = w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(false);
        let quick = self
            .app
            .get_webview_window(crate::desktop::quick::QUICK_LABEL)
            .is_some_and(|q| q.is_visible().unwrap_or(false));
        if quick {
            crate::desktop::quick::hide(&self.app);
        }
        if !shown {
            return quick;
        }
        match w.minimize() {
            Ok(()) => {
                self.hid_main.store(true, Ordering::SeqCst);
                true
            }
            Err(e) => {
                tracing::warn!("couldn't minimize Oxide: {e}");
                quick
            }
        }
    }

    fn waiting(&self, cid: &str) {
        self.update(cid, ComputerActivityState::Waiting, None);
    }

    fn ended(&self, cid: &str) {
        self.update(cid, ComputerActivityState::Ended, None);
    }
}

/// Creates the (hidden) control bar.
pub fn create_bar(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    if let Some(w) = app.get_webview_window(BAR_LABEL) {
        return Ok(w);
    }
    WebviewWindowBuilder::new(
        app,
        BAR_LABEL,
        WebviewUrl::App("index.html?window=computer".into()),
    )
    .title("Oxide is controlling your computer")
    .inner_size(BAR_WIDTH, BAR_HEIGHT)
    .resizable(false)
    .maximizable(false)
    .minimizable(false)
    .closable(false)
    .decorations(false)
    .shadow(true)
    .always_on_top(true)
    .skip_taskbar(true)
    // Clicking Stop leaves the app being used in front.
    .focusable(false)
    .focused(false)
    // Models don't see it in screenshots.
    .content_protected(true)
    .visible(false)
    .build()
}

fn show_bar(app: &AppHandle, display: Option<&str>) {
    let Some(w) = app
        .get_webview_window(BAR_LABEL)
        .or_else(|| create_bar(app).ok())
    else {
        return;
    };
    if w.is_visible().unwrap_or(false) {
        return;
    }
    place_bar(&w, display);
    if let Err(e) = w.show() {
        tracing::warn!("couldn't show the control bar: {e}");
    }
    if let Err(e) = w.set_always_on_top(true) {
        tracing::debug!("always on top: {e}");
    }
}

fn hide_bar(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(BAR_LABEL)
        && w.is_visible().unwrap_or(false)
        && let Err(e) = w.hide()
    {
        tracing::warn!("couldn't hide the control bar: {e}");
    }
}

/// Top centre of the screen models use.
fn place_bar(w: &WebviewWindow, display: Option<&str>) {
    // The screen in settings, as `oxide-computer` names it, matched by position.
    let wanted = display.and_then(|id| {
        Desktop::new()
            .displays()
            .ok()?
            .into_iter()
            .find(|d| d.id == id)
    });
    let monitors = w.available_monitors().unwrap_or_default();
    let monitor = wanted
        .and_then(|d| {
            monitors
                .iter()
                .find(|m| m.position().x == d.x && m.position().y == d.y)
                .cloned()
        })
        .or_else(|| w.primary_monitor().ok().flatten())
        .or_else(|| monitors.first().cloned());
    let Some(m) = monitor else { return };
    let scale = m.scale_factor();
    let size = PhysicalSize::new(
        (BAR_WIDTH * scale).round() as u32,
        (BAR_HEIGHT * scale).round() as u32,
    );
    let x = m.position().x + (m.size().width as i32 - size.width as i32) / 2;
    let y = m.position().y + (BAR_TOP * scale).round() as i32;
    if let Err(e) = w.set_size(size) {
        tracing::debug!("bar size: {e}");
    }
    if let Err(e) = w.set_position(PhysicalPosition::new(x, y)) {
        tracing::debug!("bar position: {e}");
    }
}

/// Stops every chat that controls the computer (the panic shortcut, the bar's Stop): actions
/// end at once, held keys come up, and the chats' replies stop.
pub fn stop_all(app: &AppHandle) -> Vec<String> {
    let Some(state) = app.try_state::<crate::state::AppState>() else {
        return Vec::new();
    };
    let Some(cu) = state.computer.clone() else {
        return Vec::new();
    };
    let chats = cu.stop_all();
    for cid in &chats {
        state.chat.stop(cid);
    }
    if !chats.is_empty() {
        tracing::info!("computer use stopped in {} chat(s)", chats.len());
        crate::desktop::show_main(app);
    }
    chats
}
