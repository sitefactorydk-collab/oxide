//! Computer use (Phase 16): runs the `computer` tool's actions on the user's screen.
//!
//! The screen and input are behind [`ComputerControl`] (the real one is `oxide-computer`,
//! wired up by the app; tests use a fake). [`ComputerUse`] keeps one session per chat that
//! controls the computer, maps screenshot pixels to the screen, checks which app an action
//! would reach (the chat service asks the user for apps not allowed yet) and refuses what is
//! never done: acting on Oxide itself or a password manager, typing into password or payment
//! fields, typing card or bank account numbers.

use std::collections::HashMap;
use std::io::Cursor;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use oxide_core::computer::{
    AppIdentity, ComputerAction, ComputerApp, ComputerDisplay, ComputerSettings, FieldInfo, Frame,
    KeyName, MouseButton, SCREENSHOTS_KEPT, ScrollDirection, app_of, blocked_app, pick_display,
    sensitive_field, sensitive_text,
};
use oxide_core::{ContentBlock, Role};
use oxide_llm::ChatMessage;
use serde_json::Value;

/// Press, release or both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Press {
    Down,
    Up,
    Click,
}

/// The screen, mouse and keyboard. Every method blocks: call them from `spawn_blocking`.
/// Coordinates are the system's (physical pixels across all screens on Windows).
pub trait ComputerControl: Send + Sync + 'static {
    fn displays(&self) -> Result<Vec<ComputerDisplay>, String>;
    /// A whole screen at its full size.
    fn capture(&self, display: &ComputerDisplay) -> Result<image::RgbaImage, String>;
    fn move_to(&self, x: i32, y: i32) -> Result<(), String>;
    fn button(&self, button: MouseButton, press: Press) -> Result<(), String>;
    fn key(&self, key: KeyName, press: Press) -> Result<(), String>;
    /// Types text (newlines and tabs as keys).
    fn text(&self, text: &str) -> Result<(), String>;
    /// Turns the wheel: positive `dy` down, positive `dx` right (in wheel clicks).
    fn scroll(&self, dx: i32, dy: i32) -> Result<(), String>;
    fn cursor(&self) -> Result<(i32, i32), String>;
    /// The app whose window is at this point.
    fn app_at(&self, x: i32, y: i32) -> Option<AppIdentity>;
    /// The app in front (that gets the keys).
    fn foreground_app(&self) -> Option<AppIdentity>;
    /// The control that has keyboard focus.
    fn focused_field(&self) -> Option<FieldInfo>;
    /// The control at a point: its name and kind ("oxide-test", "list item"), when the
    /// system can say.
    fn element_at(&self, _x: i32, _y: i32) -> Option<(String, String)> {
        None
    }
}

/// What the app does around computer use: the control bar, hiding Oxide's window, the panic
/// shortcut.
pub trait ComputerSink: Send + Sync {
    /// A chat acts (`action` is its label). With `hide`, Oxide's main window goes out of the
    /// way. Returns `true` when a window was just hidden (the caller waits for it to go).
    fn working(&self, conversation_id: &str, action: &str, hide: bool) -> bool;
    /// A chat waits for the user (an approval card): Oxide's window comes back.
    fn waiting(&self, conversation_id: &str);
    /// A chat stopped controlling the computer.
    fn ended(&self, conversation_id: &str);
}

/// A sink that does nothing (tests, builds without a UI).
pub struct NoComputerSink;

impl ComputerSink for NoComputerSink {
    fn working(&self, _: &str, _: &str, _: bool) -> bool {
        false
    }
    fn waiting(&self, _: &str) {}
    fn ended(&self, _: &str) {}
}

/// What an action returned.
#[derive(Debug, Clone, PartialEq)]
pub struct ComputerResult {
    pub text: String,
    /// A JPEG screenshot.
    pub image: Option<Vec<u8>>,
    pub is_error: bool,
}

impl ComputerResult {
    fn error(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            image: None,
            is_error: true,
        }
    }
}

/// The outcome of [`ComputerUse::act`].
#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    Done(ComputerResult),
    /// The action would reach an app this chat may not use yet: ask, then call again.
    NeedsApp(ComputerApp),
}

struct Session {
    /// Set by Stop (the panic shortcut, the bar, the chat's Stop): typing and waits end.
    stop: Arc<AtomicBool>,
    /// Keys and the button held by `left_mouse_down` / a failed action, released at the end.
    button_down: bool,
}

/// How long after an action the screenshot is taken (menus open, pages load).
const SETTLE: Duration = Duration::from_millis(700);
/// How long Oxide's window takes to get out of the way.
const HIDE_DELAY: Duration = Duration::from_millis(500);

/// Computer use for all chats.
#[derive(Clone)]
pub struct ComputerUse {
    control: Arc<dyn ComputerControl>,
    sink: Arc<dyn ComputerSink>,
    sessions: Arc<Mutex<HashMap<String, Session>>>,
    own_pid: u32,
    /// `SETTLE` and `HIDE_DELAY` (shorter in tests).
    settle: Duration,
    hide_delay: Duration,
    /// The user's folders ("Documents" → path), told to models so they don't guess paths.
    folders: Arc<Vec<(String, String)>>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl ComputerUse {
    pub fn new(control: Arc<dyn ComputerControl>, sink: Arc<dyn ComputerSink>) -> Self {
        Self {
            control,
            sink,
            sessions: Arc::default(),
            own_pid: std::process::id(),
            settle: SETTLE,
            hide_delay: HIDE_DELAY,
            folders: Arc::default(),
        }
    }

    /// The user's folders by name (home, Desktop, Documents, Downloads), from the app.
    pub fn with_folders(mut self, folders: Vec<(String, String)>) -> Self {
        self.folders = Arc::new(folders);
        self
    }

    pub fn folders(&self) -> Vec<(String, String)> {
        self.folders.as_ref().clone()
    }

    /// Waits `settle` after an action and `hide_delay` for Oxide's window to go (tests).
    pub fn with_delays(mut self, settle: Duration, hide_delay: Duration) -> Self {
        self.settle = settle;
        self.hide_delay = hide_delay;
        self
    }

    /// Treats `pid` as Oxide's own process (tests).
    pub fn with_own_pid(mut self, pid: u32) -> Self {
        self.own_pid = pid;
        self
    }

    pub fn displays(&self) -> Result<Vec<ComputerDisplay>, String> {
        self.control.displays()
    }

    /// The screen a model uses, with its grid and pointer correction.
    pub async fn frame_for(
        &self,
        settings: &ComputerSettings,
        model_id: &str,
    ) -> Result<Frame, String> {
        let f = self.frame(settings).await?;
        let tuning = settings.tuning(model_id, (f.width, f.height));
        Ok(f.with_tuning(tuning))
    }

    /// The screen a chat uses and the size of its screenshots.
    pub async fn frame(&self, settings: &ComputerSettings) -> Result<Frame, String> {
        let control = self.control.clone();
        let wanted = settings.display.clone();
        tokio::task::spawn_blocking(move || {
            let displays = control.displays()?;
            pick_display(&displays, wanted.as_deref())
                .cloned()
                .map(Frame::new)
                .ok_or_else(|| "No screen was found.".to_owned())
        })
        .await
        .unwrap_or_else(|e| Err(format!("Listing screens failed: {e}")))
    }

    /// Chats controlling the computer now.
    pub fn active(&self) -> Vec<String> {
        let mut v: Vec<String> = lock(&self.sessions).keys().cloned().collect();
        v.sort();
        v
    }

    pub fn is_active(&self, cid: &str) -> bool {
        lock(&self.sessions).contains_key(cid)
    }

    /// Stops every chat's actions at once (the panic shortcut). Returns the chats, so their
    /// replies can be stopped too.
    pub fn stop_all(&self) -> Vec<String> {
        let sessions = lock(&self.sessions);
        for s in sessions.values() {
            s.stop.store(true, Ordering::SeqCst);
        }
        let mut v: Vec<String> = sessions.keys().cloned().collect();
        v.sort();
        v
    }

    /// The chat waits for the user (its approval card shows in Oxide's window).
    pub fn waiting(&self, cid: &str) {
        if self.is_active(cid) {
            self.sink.waiting(cid);
        }
    }

    /// The chat's reply is over (or the chat is gone): held keys and buttons are released,
    /// the bar goes when no chat acts.
    pub async fn end(&self, cid: &str) {
        let Some(session) = lock(&self.sessions).remove(cid) else {
            return;
        };
        session.stop.store(true, Ordering::SeqCst);
        if session.button_down {
            let control = self.control.clone();
            let _ =
                tokio::task::spawn_blocking(move || control.button(MouseButton::Left, Press::Up))
                    .await;
        }
        self.sink.ended(cid);
    }

    fn stop_flag(&self, cid: &str) -> Arc<AtomicBool> {
        let mut sessions = lock(&self.sessions);
        let s = sessions.entry(cid.to_owned()).or_insert_with(|| Session {
            stop: Arc::new(AtomicBool::new(false)),
            button_down: false,
        });
        s.stop.clone()
    }

    fn stopped(&self, cid: &str) -> bool {
        lock(&self.sessions)
            .get(cid)
            .is_some_and(|s| s.stop.load(Ordering::SeqCst))
    }

    /// Runs one action for a chat. `allowed` says whether the chat may use an app (granted
    /// for the chat, or always in Settings); `once` is an app allowed for this action only.
    pub async fn act(
        &self,
        cid: &str,
        settings: &ComputerSettings,
        action: &ComputerAction,
        allowed: &(dyn Fn(&ComputerApp) -> bool + Sync),
        once: Option<&ComputerApp>,
    ) -> Step {
        self.act_as(cid, "", settings, action, allowed, once).await
    }

    /// [`Self::act`] for a model, with its grid and pointer correction.
    pub async fn act_as(
        &self,
        cid: &str,
        model_id: &str,
        settings: &ComputerSettings,
        action: &ComputerAction,
        allowed: &(dyn Fn(&ComputerApp) -> bool + Sync),
        once: Option<&ComputerApp>,
    ) -> Step {
        let stop = self.stop_flag(cid);
        if stop.load(Ordering::SeqCst) {
            return Step::Done(ComputerResult::error(STOPPED));
        }
        let frame = match self.frame_for(settings, model_id).await {
            Ok(f) => f,
            Err(e) => return Step::Done(ComputerResult::error(e)),
        };
        // Oxide's window gets out of the way before anything is seen or done.
        if self.sink.working(cid, &action.label(), settings.hide_oxide) {
            tokio::time::sleep(self.hide_delay).await;
        }

        if let ComputerAction::Wait { duration_ms } = action {
            if !self.sleep(cid, Duration::from_millis(*duration_ms)).await {
                return Step::Done(ComputerResult::error(STOPPED));
            }
            return Step::Done(self.screenshot(&frame, "Waited.").await);
        }

        // Which app it reaches; the user decides about apps the chat hasn't used yet. For a
        // click, also what it lands on, so the model can tell a miss.
        let mut target_app = None;
        let mut clicked: Option<(String, String)> = None;
        if !action.only_looks() {
            let control = self.control.clone();
            let (frame2, action2) = (frame.clone(), action.clone());
            let who = tokio::task::spawn_blocking(move || {
                let who = target(&*control, &frame2, &action2)?;
                let on = click_point(&*control, &frame2, &action2)
                    .and_then(|(x, y)| control.element_at(x, y));
                Ok::<_, String>((who, on))
            })
            .await
            .unwrap_or_else(|e| Err(e.to_string()));
            let who = match who {
                Ok((who, on)) => {
                    clicked = on;
                    Ok(who)
                }
                Err(e) => Err(e),
            };
            let who = match who {
                Ok(w) => w,
                Err(e) => return Step::Done(ComputerResult::error(e)),
            };
            if let Some(id) = who {
                if let Some(why) = blocked_app(&id, self.own_pid) {
                    return Step::Done(ComputerResult::error(why));
                }
                let app = app_of(&id);
                if !(allowed(&app) || once.is_some_and(|o| o.id == app.id)) {
                    return Step::NeedsApp(app);
                }
                target_app = Some(app);
            }
        }

        // Never typed: passwords and payment details.
        if let Some(text) = typed_text(action) {
            if let Some(why) = sensitive_text(&text) {
                return Step::Done(ComputerResult::error(format!(
                    "Refused: the text holds {why}. Oxide never types payment or bank details; \
                     ask the user to enter them."
                )));
            }
            let control = self.control.clone();
            let field = tokio::task::spawn_blocking(move || control.focused_field())
                .await
                .ok()
                .flatten();
            if let Some(why) = field.as_ref().and_then(sensitive_field) {
                return Step::Done(ComputerResult::error(format!(
                    "Refused: the focused field is {why}. Oxide never types into password, \
                     one-time code, payment or bank fields; ask the user to fill it in."
                )));
            }
        }

        let control = self.control.clone();
        let (frame2, action2, stop2) = (frame.clone(), action.clone(), stop.clone());
        let done =
            tokio::task::spawn_blocking(move || perform(&*control, &frame2, &action2, &stop2))
                .await
                .unwrap_or_else(|e| Err(format!("The action failed: {e}")));
        match action {
            ComputerAction::MouseDown => self.set_button(cid, true),
            ComputerAction::MouseUp => self.set_button(cid, false),
            _ => {}
        }
        let text = match done {
            Ok(t) => t,
            Err(e) => return Step::Done(ComputerResult::error(e)),
        };
        match action {
            ComputerAction::Screenshot => Step::Done(self.screenshot(&frame, "").await),
            ComputerAction::Zoom { region } => Step::Done(self.zoom(&frame, *region).await),
            _ if action.shows_result() => {
                if !self.sleep(cid, self.settle).await {
                    return Step::Done(ComputerResult::error(STOPPED));
                }
                let note = match (&clicked, &target_app) {
                    (Some((name, kind)), Some(app)) => {
                        format!(
                            "Done: clicked {} in {}.",
                            element_label(name, kind),
                            app.name
                        )
                    }
                    (Some((name, kind)), None) => {
                        format!("Done: clicked {}.", element_label(name, kind))
                    }
                    (None, Some(app)) => format!("Done ({}).", app.name),
                    (None, None) => "Done.".to_owned(),
                };
                Step::Done(self.screenshot(&frame, &note).await)
            }
            _ => Step::Done(ComputerResult {
                text: text.unwrap_or_else(|| "Done.".into()),
                image: None,
                is_error: false,
            }),
        }
    }

    fn set_button(&self, cid: &str, down: bool) {
        if let Some(s) = lock(&self.sessions).get_mut(cid) {
            s.button_down = down;
        }
    }

    /// Sleeps unless the chat is stopped. `false` when it was.
    async fn sleep(&self, cid: &str, d: Duration) -> bool {
        let step = Duration::from_millis(50);
        let mut left = d;
        while !left.is_zero() {
            if self.stopped(cid) {
                return false;
            }
            let s = left.min(step);
            tokio::time::sleep(s).await;
            left -= s;
        }
        !self.stopped(cid)
    }

    async fn screenshot(&self, frame: &Frame, note: &str) -> ComputerResult {
        let control = self.control.clone();
        let frame2 = frame.clone();
        let shot = tokio::task::spawn_blocking(move || {
            let img = control.capture(&frame2.display)?;
            let mut img = image::imageops::resize(
                &img,
                frame2.width,
                frame2.height,
                image::imageops::FilterType::Triangle,
            );
            if frame2.tuning.grid {
                draw_grid(&mut img);
            }
            encode_jpeg(&img)
        })
        .await
        .unwrap_or_else(|e| Err(e.to_string()));
        match shot {
            Ok(jpeg) => ComputerResult {
                text: if note.is_empty() {
                    format!("Screenshot ({}×{}).", frame.width, frame.height)
                } else {
                    format!("{note} Screenshot ({}×{}):", frame.width, frame.height)
                },
                image: Some(jpeg),
                is_error: false,
            },
            Err(e) => ComputerResult::error(format!(
                "{note} The screen couldn't be captured: {e}. The screen may be locked."
            )),
        }
    }

    async fn zoom(&self, frame: &Frame, region: [i32; 4]) -> ComputerResult {
        let (x, y, w, h) = match frame.region_on_screen(region) {
            Ok(r) => r,
            Err(e) => return ComputerResult::error(e),
        };
        let control = self.control.clone();
        let frame2 = frame.clone();
        let shot = tokio::task::spawn_blocking(move || {
            let img = control.capture(&frame2.display)?;
            let (iw, ih) = img.dimensions();
            // The capture may be at another scale than the screen's coordinates (Retina).
            let fx = f64::from(iw) / f64::from(frame2.display.width.max(1));
            let fy = f64::from(ih) / f64::from(frame2.display.height.max(1));
            let cx = ((f64::from(x) * fx) as u32).min(iw.saturating_sub(1));
            let cy = ((f64::from(y) * fy) as u32).min(ih.saturating_sub(1));
            let cw = ((f64::from(w) * fx).ceil() as u32).clamp(1, iw - cx);
            let ch = ((f64::from(h) * fy).ceil() as u32).clamp(1, ih - cy);
            let part = image::imageops::crop_imm(&img, cx, cy, cw, ch).to_image();
            // As large as a screenshot may be, keeping its shape.
            // The limiting side gets the screenshot's size exactly; the other side snaps to it
            // when rounding left it a pixel short (a region shaped like the screen).
            let sx = f64::from(frame2.width) / f64::from(cw);
            let sy = f64::from(frame2.height) / f64::from(ch);
            let snap = |v: f64, full: u32| {
                let v = (v.round() as u32).clamp(1, full);
                if full - v <= 1 { full } else { v }
            };
            let (ow, oh) = if sx <= sy {
                (frame2.width, snap(f64::from(ch) * sx, frame2.height))
            } else {
                (snap(f64::from(cw) * sy, frame2.width), frame2.height)
            };
            let out =
                image::imageops::resize(&part, ow, oh, image::imageops::FilterType::CatmullRom);
            encode_jpeg(&out).map(|j| (j, ow, oh))
        })
        .await
        .unwrap_or_else(|e| Err(e.to_string()));
        match shot {
            Ok((jpeg, ow, oh)) => ComputerResult {
                text: format!(
                    "Zoomed in on [{}, {}, {}, {}] ({ow}×{oh}). Coordinates stay those of the \
                     full screenshot.",
                    region[0], region[1], region[2], region[3]
                ),
                image: Some(jpeg),
                is_error: false,
            },
            Err(e) => ComputerResult::error(format!("The screen couldn't be captured: {e}")),
        }
    }
}

const STOPPED: &str = "Stopped: the user stopped computer use.";

/// Where a click lands on the screen (`None` for other actions).
fn click_point(
    control: &dyn ComputerControl,
    frame: &Frame,
    action: &ComputerAction,
) -> Option<(i32, i32)> {
    match action {
        ComputerAction::Click { at: Some(p), .. } => frame.to_screen(*p).ok(),
        ComputerAction::Click { at: None, .. } => control.cursor().ok(),
        _ => None,
    }
}

/// “oxide-test” (list item), or just the kind when it has no name.
fn element_label(name: &str, kind: &str) -> String {
    let name: String = name.trim().chars().take(60).collect();
    let kind = kind.trim();
    match (name.is_empty(), kind.is_empty()) {
        (false, false) => format!("“{name}” ({kind})"),
        (false, true) => format!("“{name}”"),
        (true, false) => format!("a {kind} with no name"),
        (true, true) => "something with no name".to_owned(),
    }
}

// ---- The grid -----------------------------------------------------------------------------

/// Grid spacing on screenshots (screenshot pixels).
pub const GRID_STEP: u32 = 100;

/// 3×5 digits, one row per byte (bit 2 = left).
const DIGITS: [[u8; 5]; 10] = [
    [7, 5, 5, 5, 7],
    [2, 6, 2, 2, 7],
    [7, 1, 7, 4, 7],
    [7, 1, 7, 1, 7],
    [5, 5, 7, 1, 1],
    [7, 4, 7, 1, 7],
    [7, 4, 7, 5, 7],
    [7, 1, 1, 1, 1],
    [7, 5, 7, 5, 7],
    [7, 5, 7, 1, 7],
];

fn blend(img: &mut image::RgbaImage, x: u32, y: u32, c: [u8; 3], alpha: u16) {
    if x >= img.width() || y >= img.height() {
        return;
    }
    let p = img.get_pixel_mut(x, y);
    for (v, c) in p.0.iter_mut().zip(c) {
        *v = ((u16::from(*v) * (256 - alpha) + u16::from(c) * alpha) >> 8) as u8;
    }
}

/// A number in 3×5 digits scaled ×2, on a white box, with its top-left at (x, y).
fn draw_number(img: &mut image::RgbaImage, x: u32, y: u32, n: u32) {
    let text = n.to_string();
    let w = text.len() as u32 * 8 + 2;
    for dy in 0..13 {
        for dx in 0..w {
            blend(img, x + dx, y + dy, [255, 255, 255], 220);
        }
    }
    for (i, ch) in text.bytes().enumerate() {
        let glyph = DIGITS[usize::from(ch - b'0')];
        for (row, bits) in glyph.iter().enumerate() {
            for col in 0..3u32 {
                if bits & (4 >> col) != 0 {
                    for (sx, sy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                        let px = x + 2 + i as u32 * 8 + col * 2 + sx;
                        let py = y + 2 + row as u32 * 2 + sy;
                        blend(img, px, py, [200, 0, 0], 256);
                    }
                }
            }
        }
    }
}

/// Faint red lines every [`GRID_STEP`] pixels, labelled with their coordinate along the top
/// and left edges, so a model can read positions instead of estimating them.
pub fn draw_grid(img: &mut image::RgbaImage) {
    let (w, h) = img.dimensions();
    for gx in (GRID_STEP..w).step_by(GRID_STEP as usize) {
        for y in 0..h {
            blend(img, gx, y, [230, 0, 0], 90);
        }
    }
    for gy in (GRID_STEP..h).step_by(GRID_STEP as usize) {
        for x in 0..w {
            blend(img, x, gy, [230, 0, 0], 90);
        }
    }
    for gx in (GRID_STEP..w).step_by(GRID_STEP as usize) {
        draw_number(img, gx + 2, 2, gx);
    }
    for gy in (GRID_STEP..h).step_by(GRID_STEP as usize) {
        draw_number(img, 2, gy + 2, gy);
    }
}

// ---- Calibration -----------------------------------------------------------------------------

/// The marker colours a calibration image uses, by the name models are asked for.
pub const MARKERS: [(&str, [u8; 3]); 6] = [
    ("red", [220, 30, 30]),
    ("green", [20, 160, 50]),
    ("blue", [30, 80, 220]),
    ("orange", [245, 140, 0]),
    ("purple", [140, 40, 180]),
    ("black", [20, 20, 20]),
];

/// A calibration image the size of the model's screenshots: a plain desktop-like picture with
/// six coloured dots. Returns the dots' centres, in [`MARKERS`] order. `round` moves the dots.
pub fn calibration_image(
    width: u32,
    height: u32,
    grid: bool,
    round: u32,
) -> (image::RgbaImage, Vec<(i32, i32)>) {
    let mut img = image::RgbaImage::from_pixel(width, height, image::Rgba([236, 238, 242, 255]));
    // A window and a taskbar, so it looks like a screen.
    for y in (height * 12 / 100)..(height * 80 / 100) {
        for x in (width * 15 / 100)..(width * 70 / 100) {
            img.put_pixel(x, y, image::Rgba([255, 255, 255, 255]));
        }
    }
    for y in height.saturating_sub(40)..height {
        for x in 0..width {
            img.put_pixel(x, y, image::Rgba([215, 220, 228, 255]));
        }
    }
    // Spread over a 3×2 layout, shifted each round; never under the taskbar.
    let cells = [(0, 0), (1, 0), (2, 0), (0, 1), (1, 1), (2, 1)];
    let r = 12i32;
    let mut centres = Vec::new();
    for i in 0..cells.len() {
        let (cx, cy) = cells[(i + round as usize * 3) % cells.len()];
        // Somewhere in the middle half of its cell, different each round.
        let jx = f64::from((round * 7 + i as u32 * 3) % 5) / 8.0;
        let jy = f64::from((round * 5 + i as u32 * 2) % 5) / 8.0;
        let fx = (f64::from(cx) + 0.25 + jx) / 3.0;
        let fy = (f64::from(cy) + 0.25 + jy) / 2.0;
        let x = (fx * f64::from(width)) as i32;
        let y = (fy * f64::from(height.saturating_sub(60))) as i32 + 10;
        let c = MARKERS[i].1;
        for dy in -r..=r {
            for dx in -r..=r {
                if dx * dx + dy * dy <= r * r {
                    let (px, py) = (x + dx, y + dy);
                    if px >= 0 && py >= 0 && (px as u32) < width && (py as u32) < height {
                        img.put_pixel(px as u32, py as u32, image::Rgba([c[0], c[1], c[2], 255]));
                    }
                }
            }
        }
        centres.push((x, y));
    }
    if grid {
        draw_grid(&mut img);
    }
    (img, centres)
}

/// The prompt for a calibration image.
pub fn calibration_prompt(width: u32, height: u32, grid: bool) -> String {
    let names: Vec<&str> = MARKERS.iter().map(|m| m.0).collect();
    format!(
        "This test image is {width}×{height} pixels, the same size as your computer-use \
         screenshots{grid}. It shows six round coloured dots: {}. For each dot, give the pixel \
         coordinates of its centre exactly as you would to click it with the computer tool \
         (x from the left edge, y from the top edge). Answer with JSON only, like \
         {{\"red\": [x, y], \"green\": [x, y], ...}}.",
        names.join(", "),
        grid = if grid {
            ", with the same grid (lines every 100 pixels, labelled)"
        } else {
            ""
        },
    )
}

/// The dots a model found, by name, from its answer (JSON somewhere in the text).
pub fn parse_calibration_answer(text: &str) -> Vec<(String, (i32, i32))> {
    let (Some(a), Some(b)) = (text.find('{'), text.rfind('}')) else {
        return Vec::new();
    };
    if b <= a {
        return Vec::new();
    }
    let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&text[a..=b]) else {
        return Vec::new();
    };
    let num = |v: &Value| -> Option<i32> {
        v.as_f64()
            .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
            .map(|f| f.round() as i32)
    };
    map.iter()
        .filter_map(|(k, v)| {
            let p = match v {
                Value::Array(a) if a.len() == 2 => (num(&a[0])?, num(&a[1])?),
                Value::Object(o) => (num(o.get("x")?)?, num(o.get("y")?)?),
                _ => return None,
            };
            Some((k.trim().to_lowercase(), p))
        })
        .collect()
}

/// The text an action would type, for the safety checks.
fn typed_text(action: &ComputerAction) -> Option<String> {
    match action {
        ComputerAction::Type { text } => Some(text.clone()),
        ComputerAction::Key { keys, .. } => {
            // Keys that enter characters (alone or with Shift), or paste.
            let enters = keys.iter().any(|combo| {
                let mods: Vec<&KeyName> = combo.iter().filter(|k| k.is_modifier()).collect();
                let paste = (combo.contains(&KeyName::Control) || combo.contains(&KeyName::Meta))
                    && combo.contains(&KeyName::Char('v'))
                    || combo.contains(&KeyName::Shift) && combo.contains(&KeyName::Insert);
                paste
                    || (mods.iter().all(|k| **k == KeyName::Shift)
                        && combo.iter().any(|k| k.enters_text()))
            });
            enters.then(String::new)
        }
        _ => None,
    }
}

/// The app an action reaches: the window under the point for the mouse, the app in front
/// for the keyboard.
fn target(
    control: &dyn ComputerControl,
    frame: &Frame,
    action: &ComputerAction,
) -> Result<Option<AppIdentity>, String> {
    let at = |p: Option<(i32, i32)>| -> Result<Option<AppIdentity>, String> {
        let (x, y) = match p {
            Some(p) => frame.to_screen(p)?,
            None => control.cursor()?,
        };
        Ok(control.app_at(x, y))
    };
    match action {
        ComputerAction::Click { at: p, .. } | ComputerAction::Scroll { at: p, .. } => at(*p),
        ComputerAction::Drag { from, .. } => at(Some(*from)),
        ComputerAction::MouseDown | ComputerAction::MouseUp => at(None),
        ComputerAction::Type { .. }
        | ComputerAction::Key { .. }
        | ComputerAction::HoldKey { .. } => {
            Ok(control.foreground_app().or_else(|| at(None).ok().flatten()))
        }
        _ => Ok(None),
    }
}

/// Holds `mods` while `f` runs (released even when it fails).
fn with_mods(
    control: &dyn ComputerControl,
    mods: &[KeyName],
    f: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    let mut held = Vec::new();
    let mut r = Ok(());
    for m in mods {
        match control.key(*m, Press::Down) {
            Ok(()) => held.push(*m),
            Err(e) => {
                r = Err(e);
                break;
            }
        }
    }
    if r.is_ok() {
        r = f();
    }
    for m in held.iter().rev() {
        let _ = control.key(*m, Press::Up);
    }
    r
}

fn pause(ms: u64) {
    std::thread::sleep(Duration::from_millis(ms));
}

/// Does an input action. Returns text for actions that answer with text.
pub fn perform(
    control: &dyn ComputerControl,
    frame: &Frame,
    action: &ComputerAction,
    stop: &AtomicBool,
) -> Result<Option<String>, String> {
    let stopped = || stop.load(Ordering::SeqCst);
    match action {
        ComputerAction::Screenshot | ComputerAction::Zoom { .. } | ComputerAction::Wait { .. } => {
            Ok(None)
        }
        ComputerAction::CursorPosition => {
            let p = control.cursor()?;
            Ok(Some(match frame.to_model(p) {
                Some((x, y)) => format!("The mouse pointer is at ({x}, {y})."),
                None => "The mouse pointer is on another screen.".into(),
            }))
        }
        ComputerAction::MouseMove { to } => {
            let (x, y) = frame.to_screen(*to)?;
            control.move_to(x, y)?;
            Ok(None)
        }
        ComputerAction::Click {
            button,
            count,
            at,
            modifiers,
        } => {
            if let Some(p) = at {
                let (x, y) = frame.to_screen(*p)?;
                control.move_to(x, y)?;
                pause(30);
            }
            with_mods(control, modifiers, || {
                for i in 0..*count {
                    if i > 0 {
                        pause(40);
                    }
                    control.button(*button, Press::Click)?;
                }
                Ok(())
            })?;
            Ok(None)
        }
        ComputerAction::Drag {
            from,
            to,
            modifiers,
        } => {
            let (fx, fy) = frame.to_screen(*from)?;
            let (tx, ty) = frame.to_screen(*to)?;
            control.move_to(fx, fy)?;
            pause(50);
            with_mods(control, modifiers, || {
                control.button(MouseButton::Left, Press::Down)?;
                let moved = (|| {
                    const STEPS: i32 = 12;
                    for i in 1..=STEPS {
                        let x = fx + (tx - fx) * i / STEPS;
                        let y = fy + (ty - fy) * i / STEPS;
                        control.move_to(x, y)?;
                        pause(15);
                    }
                    Ok::<(), String>(())
                })();
                pause(50);
                let up = control.button(MouseButton::Left, Press::Up);
                moved.and(up)
            })?;
            Ok(None)
        }
        ComputerAction::MouseDown => {
            control.button(MouseButton::Left, Press::Down)?;
            Ok(None)
        }
        ComputerAction::MouseUp => {
            control.button(MouseButton::Left, Press::Up)?;
            Ok(None)
        }
        ComputerAction::Scroll {
            direction,
            amount,
            at,
            modifiers,
        } => {
            if let Some(p) = at {
                let (x, y) = frame.to_screen(*p)?;
                control.move_to(x, y)?;
                pause(30);
            }
            let n = *amount as i32;
            let (dx, dy) = match direction {
                ScrollDirection::Up => (0, -n),
                ScrollDirection::Down => (0, n),
                ScrollDirection::Left => (-n, 0),
                ScrollDirection::Right => (n, 0),
            };
            with_mods(control, modifiers, || control.scroll(dx, dy))?;
            Ok(None)
        }
        ComputerAction::Type { text } => {
            // In small pieces, so Stop takes effect at once.
            let chars: Vec<char> = text.chars().collect();
            for chunk in chars.chunks(20) {
                if stopped() {
                    return Err(STOPPED.into());
                }
                let piece: String = chunk.iter().collect();
                control.text(&piece)?;
                pause(10);
            }
            Ok(None)
        }
        ComputerAction::Key { keys, repeat, .. } => {
            for _ in 0..*repeat {
                for combo in keys {
                    if stopped() {
                        return Err(STOPPED.into());
                    }
                    press_combo(control, combo)?;
                    pause(30);
                }
            }
            Ok(None)
        }
        ComputerAction::HoldKey {
            keys, duration_ms, ..
        } => {
            let mut held = Vec::new();
            let mut r = Ok(());
            for k in keys {
                match control.key(*k, Press::Down) {
                    Ok(()) => held.push(*k),
                    Err(e) => {
                        r = Err(e);
                        break;
                    }
                }
            }
            if r.is_ok() {
                let mut left = *duration_ms;
                while left > 0 && !stopped() {
                    let s = left.min(50);
                    pause(s);
                    left -= s;
                }
            }
            for k in held.iter().rev() {
                let _ = control.key(*k, Press::Up);
            }
            r.map(|()| None)
        }
    }
}

/// Presses a combination: keys down in order, up in reverse.
fn press_combo(control: &dyn ComputerControl, combo: &[KeyName]) -> Result<(), String> {
    if let [one] = combo {
        return control.key(*one, Press::Click);
    }
    let mut held = Vec::new();
    let mut r = Ok(());
    for k in combo {
        match control.key(*k, Press::Down) {
            Ok(()) => held.push(*k),
            Err(e) => {
                r = Err(e);
                break;
            }
        }
    }
    for k in held.iter().rev() {
        let _ = control.key(*k, Press::Up);
    }
    r
}

/// A screenshot as JPEG (smaller than PNG; screenshots are many).
pub fn encode_jpeg(img: &image::RgbaImage) -> Result<Vec<u8>, String> {
    let rgb = image::DynamicImage::ImageRgba8(img.clone()).to_rgb8();
    let mut out = Cursor::new(Vec::new());
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 80)
        .encode_image(&rgb)
        .map_err(|e| format!("couldn't encode the screenshot: {e}"))?;
    Ok(out.into_inner())
}

/// Keeps only the newest screenshots of computer calls in a request (older ones become a
/// short note): each costs about 1,400 tokens, and only the latest show the screen as it is.
pub fn trim_screenshots(messages: &mut [ChatMessage], keep: usize) {
    use std::collections::HashSet;
    let ids: HashSet<String> = messages
        .iter()
        .filter(|m| m.role == Role::Assistant)
        .flat_map(|m| m.content.iter())
        .filter_map(|b| match b {
            ContentBlock::ToolUse { id, name, .. }
                if name == oxide_core::computer::COMPUTER_TOOL =>
            {
                Some(id.clone())
            }
            _ => None,
        })
        .collect();
    if ids.is_empty() {
        return;
    }
    let mut seen = 0usize;
    for m in messages.iter_mut().rev() {
        for b in m.content.iter_mut().rev() {
            if let ContentBlock::ToolResult {
                tool_use_id,
                content,
                ..
            } = b
                && ids.contains(tool_use_id)
            {
                for c in content.iter_mut().rev() {
                    if matches!(c, ContentBlock::Image { .. }) {
                        seen += 1;
                        if seen > keep {
                            *c = ContentBlock::text(
                                "[An older screenshot was removed to save context.]",
                            );
                        }
                    }
                }
            }
        }
    }
}

/// The default number kept by [`trim_screenshots`].
pub fn trim_screenshots_default(messages: &mut [ChatMessage]) {
    trim_screenshots(messages, SCREENSHOTS_KEPT);
}

/// A stand-in computer for tests: one 2560×1440 screen, apps by region, a log of input.
#[cfg(test)]
pub mod testing {
    use super::*;

    #[derive(Default)]
    pub struct FakeComputer {
        pub log: Mutex<Vec<String>>,
        /// The app at every point and in front.
        pub app: Mutex<Option<AppIdentity>>,
        pub field: Mutex<Option<FieldInfo>>,
        /// The control under every point (name, kind).
        pub element: Mutex<Option<(String, String)>>,
        pub cursor: Mutex<(i32, i32)>,
        /// Each `text` call blocks this long (to test Stop).
        pub type_delay_ms: Mutex<u64>,
    }

    impl FakeComputer {
        pub fn with_app(exe: &str, name: &str, pid: u32) -> Self {
            let f = Self::default();
            *lock(&f.app) = Some(AppIdentity {
                exe: exe.into(),
                name: name.into(),
                title: String::new(),
                pid,
            });
            f
        }

        pub fn log(&self) -> Vec<String> {
            lock(&self.log).clone()
        }

        fn push(&self, s: String) {
            lock(&self.log).push(s);
        }
    }

    pub fn screen() -> ComputerDisplay {
        ComputerDisplay {
            id: "DISPLAY1".into(),
            name: "Test screen".into(),
            x: 0,
            y: 0,
            width: 2560,
            height: 1440,
            scale_factor: 1.0,
            primary: true,
        }
    }

    impl ComputerControl for FakeComputer {
        fn displays(&self) -> Result<Vec<ComputerDisplay>, String> {
            Ok(vec![screen()])
        }
        fn capture(&self, d: &ComputerDisplay) -> Result<image::RgbaImage, String> {
            self.push("capture".into());
            Ok(image::RgbaImage::from_pixel(
                d.width,
                d.height,
                image::Rgba([240, 240, 240, 255]),
            ))
        }
        fn move_to(&self, x: i32, y: i32) -> Result<(), String> {
            *lock(&self.cursor) = (x, y);
            self.push(format!("move {x},{y}"));
            Ok(())
        }
        fn button(&self, b: MouseButton, p: Press) -> Result<(), String> {
            self.push(format!("button {b:?} {p:?}"));
            Ok(())
        }
        fn key(&self, k: KeyName, p: Press) -> Result<(), String> {
            self.push(format!("key {k:?} {p:?}"));
            Ok(())
        }
        fn text(&self, t: &str) -> Result<(), String> {
            let d = *lock(&self.type_delay_ms);
            if d > 0 {
                std::thread::sleep(Duration::from_millis(d));
            }
            self.push(format!("text {t}"));
            Ok(())
        }
        fn scroll(&self, dx: i32, dy: i32) -> Result<(), String> {
            self.push(format!("scroll {dx},{dy}"));
            Ok(())
        }
        fn cursor(&self) -> Result<(i32, i32), String> {
            Ok(*lock(&self.cursor))
        }
        fn app_at(&self, _: i32, _: i32) -> Option<AppIdentity> {
            lock(&self.app).clone()
        }
        fn foreground_app(&self) -> Option<AppIdentity> {
            lock(&self.app).clone()
        }
        fn focused_field(&self) -> Option<FieldInfo> {
            lock(&self.field).clone()
        }
        fn element_at(&self, _: i32, _: i32) -> Option<(String, String)> {
            lock(&self.element).clone()
        }
    }

    /// Records what the sink was told.
    #[derive(Default)]
    pub struct SinkLog(pub Mutex<Vec<String>>);

    impl ComputerSink for SinkLog {
        fn working(&self, cid: &str, action: &str, hide: bool) -> bool {
            lock(&self.0).push(format!("working {cid} {action} hide={hide}"));
            false
        }
        fn waiting(&self, cid: &str) {
            lock(&self.0).push(format!("waiting {cid}"));
        }
        fn ended(&self, cid: &str) {
            lock(&self.0).push(format!("ended {cid}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::*;
    use super::*;
    use oxide_core::computer::parse_action;
    use serde_json::json;

    fn cu(fake: Arc<FakeComputer>) -> (ComputerUse, Arc<SinkLog>) {
        let sink = Arc::new(SinkLog::default());
        (ComputerUse::new(fake, sink.clone()).with_own_pid(1), sink)
    }

    fn act_json(v: serde_json::Value) -> ComputerAction {
        parse_action(&v).expect("action")
    }

    fn yes(_: &ComputerApp) -> bool {
        true
    }
    fn no(_: &ComputerApp) -> bool {
        false
    }

    #[tokio::test]
    async fn clicks_land_on_the_screen_and_come_back_with_a_screenshot() {
        let fake = Arc::new(FakeComputer::with_app("notepad.exe", "Notepad", 50));
        let (cu, sink) = cu(fake.clone());
        let s = ComputerSettings::default();
        let a = act_json(json!({"action": "left_click", "coordinate": [683, 384]}));
        let Step::Done(r) = cu.act("c1", &s, &a, &yes, None).await else {
            panic!("needs no app")
        };
        assert!(!r.is_error, "{}", r.text);
        assert!(r.text.contains("Notepad"), "{}", r.text);
        let jpeg = r.image.expect("screenshot");
        let img = image::load_from_memory(&jpeg).expect("jpeg");
        assert_eq!((img.width(), img.height()), (1366, 768));
        assert_eq!(
            fake.log(),
            vec!["move 1280,720", "button Left Click", "capture"]
        );
        assert!(cu.is_active("c1"));
        assert!(lock(&sink.0)[0].starts_with("working c1 Clicking at (683, 384) hide=true"));
        cu.end("c1").await;
        assert!(!cu.is_active("c1"));
        assert_eq!(lock(&sink.0).last().map(String::as_str), Some("ended c1"));
    }

    #[tokio::test]
    async fn clicks_say_what_they_reached_and_models_get_their_grid_and_correction() {
        use oxide_core::computer::{AxisFit, ComputerModelSettings, PointerCalibration};
        let fake = Arc::new(FakeComputer::with_app("notepad.exe", "Notepad", 50));
        *lock(&fake.element) = Some(("File name:".into(), "edit".into()));
        let (cu, _) = cu(fake.clone());
        let calibration = PointerCalibration {
            x: AxisFit::EXACT,
            y: AxisFit {
                scale: 10_000,
                offset: -55,
            },
            markers: 6,
            shown: 6,
            error_before: 55,
            error_after: 1,
            width: 1366,
            height: 768,
            grid: true,
            at: 1,
        };
        let s = ComputerSettings {
            models: vec![ComputerModelSettings {
                model: "strata/swift".into(),
                grid: true,
                calibration: Some(calibration),
            }],
            ..ComputerSettings::default()
        };
        // The model aims 55 px too high: its (683, 329) means (683, 384).
        let a = act_json(json!({"action": "left_click", "coordinate": [683, 329]}));
        let Step::Done(r) = cu.act_as("c1", "strata/swift", &s, &a, &yes, None).await else {
            panic!("needs no app")
        };
        assert_eq!(
            r.text,
            "Done: clicked “File name:” (edit) in Notepad. Screenshot (1366×768):"
        );
        assert_eq!(fake.log()[0], "move 1280,720");
        // Its screenshots carry the grid: the line at x = 100 is red over the grey screen.
        let img = image::load_from_memory(&r.image.expect("screenshot"))
            .expect("jpeg")
            .to_rgba8();
        let line = img.get_pixel(100, 450);
        let plain = img.get_pixel(150, 450);
        assert!(
            line[0] > line[1] + 20 && plain[0].abs_diff(plain[1]) < 10,
            "{line:?} {plain:?}"
        );
        // The pointer is reported in the model's terms.
        let pos = act_json(json!({"action": "cursor_position"}));
        let Step::Done(r) = cu.act_as("c1", "strata/swift", &s, &pos, &yes, None).await else {
            panic!("done")
        };
        assert!(r.text.contains("(683, 329)"), "{}", r.text);
        // Another model: no grid, no correction, and the click names what it hit.
        *lock(&fake.element) = Some((String::new(), "pane".into()));
        let Step::Done(r) = cu
            .act_as("c1", "anthropic/claude", &s, &a, &yes, None)
            .await
        else {
            panic!("needs no app")
        };
        assert!(
            r.text
                .starts_with("Done: clicked a pane with no name in Notepad."),
            "{}",
            r.text
        );
        assert!(
            fake.log().contains(&"move 1280,617".to_owned()),
            "{:?}",
            fake.log()
        );
        cu.end("c1").await;
    }

    #[test]
    fn calibration_images_have_their_dots_where_they_say() {
        for round in 0..2 {
            let (img, centres) = calibration_image(1366, 768, round == 1, round);
            assert_eq!(centres.len(), MARKERS.len());
            for (i, &(x, y)) in centres.iter().enumerate() {
                assert!(x > 20 && y > 20 && x < 1346 && y < 728, "{x},{y}");
                let p = img.get_pixel(x as u32, y as u32);
                let want = MARKERS[i].1;
                assert!(
                    (0..3).all(|c| p[c].abs_diff(want[c]) < 40),
                    "dot {i} at {x},{y} is {p:?}"
                );
            }
            // The dots are apart from each other.
            for a in 0..centres.len() {
                for b in a + 1..centres.len() {
                    let d = f64::from(centres[a].0 - centres[b].0)
                        .hypot(f64::from(centres[a].1 - centres[b].1));
                    assert!(d > 80.0, "{centres:?}");
                }
            }
        }
        let mut found = parse_calibration_answer(
            "Sure! ```json\n{\"Red\": [10, 20], \"green\": {\"x\": 30, \"y\": \"40\"}, \"blue\": [1]}\n```",
        );
        found.sort();
        assert_eq!(
            found,
            vec![("green".to_owned(), (30, 40)), ("red".to_owned(), (10, 20))]
        );
        assert!(parse_calibration_answer("no idea").is_empty());
        assert!(calibration_prompt(1366, 768, true).contains("grid"));
    }

    #[tokio::test]
    async fn apps_not_allowed_yet_are_asked_about_and_blocked_ones_refused() {
        let fake = Arc::new(FakeComputer::with_app("notepad.exe", "Notepad", 50));
        let (cu, _) = cu(fake.clone());
        let s = ComputerSettings::default();
        let a = act_json(json!({"action": "type", "text": "hi"}));
        assert_eq!(
            cu.act("c1", &s, &a, &no, None).await,
            Step::NeedsApp(ComputerApp {
                id: "notepad.exe".into(),
                name: "Notepad".into()
            })
        );
        assert!(
            fake.log().is_empty(),
            "nothing done before the user says yes"
        );
        // Allowed for this action only.
        let once = ComputerApp {
            id: "notepad.exe".into(),
            name: "Notepad".into(),
        };
        assert!(matches!(
            cu.act("c1", &s, &a, &no, Some(&once)).await,
            Step::Done(ComputerResult {
                is_error: false,
                ..
            })
        ));
        // Looking needs no app.
        let shot = act_json(json!({"action": "screenshot"}));
        assert!(matches!(
            cu.act("c1", &s, &shot, &no, None).await,
            Step::Done(ComputerResult {
                is_error: false,
                image: Some(_),
                ..
            })
        ));
        // Oxide's own window and password managers: never.
        *lock(&fake.app) = Some(AppIdentity {
            exe: "oxide.exe".into(),
            name: "Oxide".into(),
            title: String::new(),
            pid: 1,
        });
        let Step::Done(r) = cu.act("c1", &s, &a, &yes, None).await else {
            panic!("refused without asking")
        };
        assert!(r.is_error && r.text.contains("Oxide"), "{}", r.text);
        *lock(&fake.app) = Some(AppIdentity {
            exe: "KeePassXC.exe".into(),
            name: "KeePassXC".into(),
            title: String::new(),
            pid: 9,
        });
        let Step::Done(r) = cu.act("c1", &s, &a, &yes, None).await else {
            panic!("refused without asking")
        };
        assert!(r.is_error && r.text.contains("passwords"), "{}", r.text);
    }

    #[tokio::test]
    async fn password_fields_and_card_numbers_are_never_typed() {
        let fake = Arc::new(FakeComputer::with_app("chrome.exe", "Google Chrome", 50));
        let (cu, _) = cu(fake.clone());
        let s = ComputerSettings::default();
        *lock(&fake.field) = Some(FieldInfo {
            is_password: true,
            ..FieldInfo::default()
        });
        let a = act_json(json!({"action": "type", "text": "hunter2"}));
        let Step::Done(r) = cu.act("c1", &s, &a, &yes, None).await else {
            panic!("done")
        };
        assert!(r.is_error && r.text.contains("password"), "{}", r.text);
        // Pasting counts as typing; Enter doesn't.
        let paste = act_json(json!({"action": "key", "text": "ctrl+v"}));
        let Step::Done(r) = cu.act("c1", &s, &paste, &yes, None).await else {
            panic!("done")
        };
        assert!(r.is_error, "{}", r.text);
        let enter = act_json(json!({"action": "key", "text": "Return"}));
        let Step::Done(r) = cu.act("c1", &s, &enter, &yes, None).await else {
            panic!("done")
        };
        assert!(!r.is_error, "{}", r.text);
        *lock(&fake.field) = None;
        let card = act_json(json!({"action": "type", "text": "4111 1111 1111 1111"}));
        let Step::Done(r) = cu.act("c1", &s, &card, &yes, None).await else {
            panic!("done")
        };
        assert!(r.is_error && r.text.contains("card"), "{}", r.text);
        assert!(
            !fake.log().iter().any(|l| l.starts_with("text")),
            "nothing typed: {:?}",
            fake.log()
        );
    }

    #[tokio::test]
    async fn keys_drags_scrolls_and_the_pointer() {
        let fake = Arc::new(FakeComputer::with_app("notepad.exe", "Notepad", 50));
        let (cu, _) = cu(fake.clone());
        let s = ComputerSettings::default();
        for v in [
            json!({"action": "key", "text": "ctrl+s"}),
            json!({"action": "scroll", "scroll_direction": "up", "scroll_amount": 2, "text": "ctrl"}),
            json!({"action": "left_click_drag", "start_coordinate": [0, 0], "coordinate": [683, 0]}),
        ] {
            let Step::Done(r) = cu.act("c1", &s, &act_json(v), &yes, None).await else {
                panic!("done")
            };
            assert!(!r.is_error, "{}", r.text);
        }
        let log = fake.log();
        let keys: Vec<&String> = log.iter().filter(|l| l.starts_with("key")).collect();
        assert_eq!(
            keys[..4],
            [
                "key Control Down",
                "key Char('s') Down",
                "key Char('s') Up",
                "key Control Up"
            ]
        );
        assert!(log.contains(&"scroll 0,-2".to_owned()), "{log:?}");
        assert!(log.contains(&"move 1280,0".to_owned()), "{log:?}");
        assert!(log.contains(&"button Left Down".to_owned()));
        assert!(log.contains(&"button Left Up".to_owned()));
        let Step::Done(r) = cu
            .act(
                "c1",
                &s,
                &act_json(json!({"action": "cursor_position"})),
                &yes,
                None,
            )
            .await
        else {
            panic!("done")
        };
        assert_eq!(r.text, "The mouse pointer is at (683, 0).");
        let Step::Done(r) = cu
            .act(
                "c1",
                &s,
                &act_json(json!({"action": "left_click", "coordinate": [1366, 0]})),
                &yes,
                None,
            )
            .await
        else {
            panic!("done")
        };
        assert!(r.is_error && r.text.contains("outside"), "{}", r.text);
        let Step::Done(r) = cu
            .act(
                "c1",
                &s,
                &act_json(json!({"action": "zoom", "region": [0, 0, 683, 384]})),
                &yes,
                None,
            )
            .await
        else {
            panic!("done")
        };
        let img = image::load_from_memory(r.image.as_deref().expect("image")).expect("jpeg");
        assert_eq!((img.width(), img.height()), (1366, 768));
    }

    #[tokio::test]
    async fn stop_all_ends_typing_at_once() {
        let fake = Arc::new(FakeComputer::with_app("notepad.exe", "Notepad", 50));
        *lock(&fake.type_delay_ms) = 30;
        let (cu, _) = cu(fake.clone());
        let s = ComputerSettings::default();
        let long = "x".repeat(2_000);
        let a = act_json(json!({"action": "type", "text": long}));
        let cu2 = cu.clone();
        let task = tokio::spawn(async move { cu2.act("c1", &s, &a, &yes, None).await });
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert_eq!(cu.stop_all(), vec!["c1".to_owned()]);
        let Step::Done(r) = tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .expect("stops quickly")
            .expect("joins")
        else {
            panic!("done")
        };
        assert!(r.is_error && r.text.starts_with("Stopped"), "{}", r.text);
        let typed = fake.log().iter().filter(|l| l.starts_with("text")).count();
        assert!(typed < 20, "stopped after {typed} pieces of 100");
        // Further actions in that chat are refused until its reply ends.
        let shot = act_json(json!({"action": "screenshot"}));
        assert!(matches!(
            cu.act("c1", &ComputerSettings::default(), &shot, &yes, None)
                .await,
            Step::Done(ComputerResult { is_error: true, .. })
        ));
        cu.end("c1").await;
        assert!(cu.active().is_empty());
    }

    #[test]
    fn only_the_newest_screenshots_stay_in_a_request() {
        let img = || ContentBlock::Image {
            media_type: "image/jpeg".into(),
            source: oxide_core::MediaSource::Base64 {
                data: "aGk=".into(),
            },
        };
        let mut messages = Vec::new();
        for i in 0..5 {
            messages.push(ChatMessage {
                role: Role::Assistant,
                content: vec![ContentBlock::ToolUse {
                    id: format!("t{i}"),
                    name: "computer".into(),
                    input_json: "{\"action\":\"screenshot\"}".into(),
                    server: None,
                    tool: None,
                }],
            });
            messages.push(ChatMessage {
                role: Role::User,
                content: vec![ContentBlock::ToolResult {
                    tool_use_id: format!("t{i}"),
                    content: vec![ContentBlock::text("Screenshot"), img()],
                    is_error: false,
                }],
            });
        }
        // An image from another tool stays.
        messages.push(ChatMessage {
            role: Role::User,
            content: vec![img()],
        });
        trim_screenshots(&mut messages, 3);
        let images = |m: &ChatMessage| match &m.content[0] {
            ContentBlock::ToolResult { content, .. } => content
                .iter()
                .filter(|c| matches!(c, ContentBlock::Image { .. }))
                .count(),
            ContentBlock::Image { .. } => 1,
            _ => 0,
        };
        let counts: Vec<usize> = messages.iter().map(images).collect();
        assert_eq!(counts, vec![0, 0, 0, 0, 0, 1, 0, 1, 0, 1, 1]);
    }
}
