//! The real screen, mouse and keyboard for computer use (Phase 16).
//!
//! [`Desktop`] lists screens and captures them with `xcap` on every platform. Input (`enigo`
//! for buttons, keys, text and the wheel; `SetCursorPos` for moves, because enigo's absolute
//! moves only map the main screen), which app a point or the keyboard reaches, and the focused
//! control (UI Automation) are Windows-only for now: elsewhere they fail and
//! [`Desktop::supported`] is `false`, so the app doesn't offer computer use.
//!
//! Coordinates are the system's: physical pixels across all screens on Windows (Tauri makes
//! the process per-monitor DPI aware, and xcap reports screens in the same pixels).
//! Every method blocks: call them from `spawn_blocking`.

use oxide_core::computer::{AppIdentity, ComputerDisplay, FieldInfo, KeyName, MouseButton};

/// Press, release or both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Press {
    Down,
    Up,
    Click,
}

/// The computer Oxide runs on.
pub struct Desktop {
    #[cfg(windows)]
    input: std::sync::Mutex<Option<enigo::Enigo>>,
}

impl Default for Desktop {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(not(windows))]
const UNSUPPORTED: &str = "Computer use isn't available on this system yet (Windows only).";

impl Desktop {
    pub fn new() -> Self {
        // Physical pixels everywhere (Tauri already makes the app per-monitor DPI aware;
        // this covers tests and other hosts).
        #[cfg(windows)]
        win::dpi_aware();
        Self {
            #[cfg(windows)]
            input: std::sync::Mutex::new(None),
        }
    }

    /// Whether computer use works here (input and the safety checks).
    pub fn supported() -> bool {
        cfg!(windows)
    }

    /// The screens, the main one first.
    pub fn displays(&self) -> Result<Vec<ComputerDisplay>, String> {
        let monitors = xcap::Monitor::all().map_err(|e| format!("listing screens failed: {e}"))?;
        let mut out = Vec::new();
        for m in monitors {
            let (Ok(x), Ok(y), Ok(width), Ok(height)) = (m.x(), m.y(), m.width(), m.height())
            else {
                continue;
            };
            if width == 0 || height == 0 {
                continue;
            }
            let id = m
                .name()
                .ok()
                .filter(|n| !n.trim().is_empty())
                .unwrap_or_else(|| format!("screen-{}", m.id().unwrap_or_default()));
            let name = m
                .friendly_name()
                .ok()
                .map(|n| n.trim().to_owned())
                .filter(|n| !n.is_empty() && !n.starts_with("Unknown Monitor"))
                .unwrap_or_else(|| "Screen".to_owned());
            out.push(ComputerDisplay {
                id,
                name,
                x,
                y,
                width,
                height,
                scale_factor: m.scale_factor().unwrap_or(1.0),
                primary: m.is_primary().unwrap_or(false),
            });
        }
        // The main screen first, then left to right.
        out.sort_by_key(|d| (!d.primary, d.x, d.y));
        if out.is_empty() {
            return Err("no screen was found".into());
        }
        Ok(out)
    }

    /// A whole screen at its full size.
    pub fn capture(&self, display: &ComputerDisplay) -> Result<image::RgbaImage, String> {
        let monitors = xcap::Monitor::all().map_err(|e| format!("listing screens failed: {e}"))?;
        let monitor = monitors
            .into_iter()
            .find(|m| m.name().ok().as_deref() == Some(display.id.as_str()))
            .or_else(|| {
                // The id changed (screens were rearranged): the screen at the same place.
                xcap::Monitor::from_point(display.x + 1, display.y + 1).ok()
            })
            .ok_or_else(|| format!("the screen {} is gone", display.name))?;
        let shot = monitor.capture_image().map_err(|e| e.to_string())?;
        let (w, h) = (shot.width(), shot.height());
        // Rebuilt with this crate's `image`, so xcap's own `image` version doesn't matter.
        image::RgbaImage::from_raw(w, h, shot.into_raw())
            .ok_or_else(|| "unexpected pixel buffer size".to_owned())
    }

    pub fn move_to(&self, x: i32, y: i32) -> Result<(), String> {
        #[cfg(windows)]
        {
            win::move_to(x, y)
        }
        #[cfg(not(windows))]
        {
            let _ = (x, y);
            Err(UNSUPPORTED.into())
        }
    }

    pub fn cursor(&self) -> Result<(i32, i32), String> {
        #[cfg(windows)]
        {
            win::cursor()
        }
        #[cfg(not(windows))]
        {
            Err(UNSUPPORTED.into())
        }
    }

    pub fn button(&self, button: MouseButton, press: Press) -> Result<(), String> {
        #[cfg(windows)]
        {
            use enigo::Mouse as _;
            let b = match button {
                MouseButton::Left => enigo::Button::Left,
                MouseButton::Right => enigo::Button::Right,
                MouseButton::Middle => enigo::Button::Middle,
            };
            self.with_input(|e| e.button(b, direction(press)))
        }
        #[cfg(not(windows))]
        {
            let _ = (button, press);
            Err(UNSUPPORTED.into())
        }
    }

    pub fn key(&self, key: KeyName, press: Press) -> Result<(), String> {
        #[cfg(windows)]
        {
            use enigo::Keyboard as _;
            let k = enigo_key(key);
            self.with_input(|e| e.key(k, direction(press)))
        }
        #[cfg(not(windows))]
        {
            let _ = (key, press);
            Err(UNSUPPORTED.into())
        }
    }

    /// Types text (newlines and tabs as the Enter and Tab keys).
    pub fn text(&self, text: &str) -> Result<(), String> {
        #[cfg(windows)]
        {
            use enigo::Keyboard as _;
            // A Windows line break is one Enter.
            let text = text.replace("\r\n", "\n").replace('\r', "\n");
            self.with_input(|e| e.text(&text))
        }
        #[cfg(not(windows))]
        {
            let _ = text;
            Err(UNSUPPORTED.into())
        }
    }

    /// Turns the wheel: positive `dy` down, positive `dx` right (in wheel clicks).
    pub fn scroll(&self, dx: i32, dy: i32) -> Result<(), String> {
        #[cfg(windows)]
        {
            use enigo::Mouse as _;
            self.with_input(|e| {
                if dy != 0 {
                    e.scroll(dy, enigo::Axis::Vertical)?;
                }
                if dx != 0 {
                    e.scroll(dx, enigo::Axis::Horizontal)?;
                }
                Ok(())
            })
        }
        #[cfg(not(windows))]
        {
            let _ = (dx, dy);
            Err(UNSUPPORTED.into())
        }
    }

    /// The app whose window is at this point.
    pub fn app_at(&self, x: i32, y: i32) -> Option<AppIdentity> {
        #[cfg(windows)]
        {
            win::app_at(x, y)
        }
        #[cfg(not(windows))]
        {
            let _ = (x, y);
            None
        }
    }

    /// The app in front (that gets the keys).
    pub fn foreground_app(&self) -> Option<AppIdentity> {
        #[cfg(windows)]
        {
            win::foreground_app()
        }
        #[cfg(not(windows))]
        {
            None
        }
    }

    /// The control that has keyboard focus (`None` when UI Automation doesn't answer in
    /// time).
    pub fn focused_field(&self) -> Option<FieldInfo> {
        #[cfg(windows)]
        {
            win::focused_field()
        }
        #[cfg(not(windows))]
        {
            None
        }
    }

    /// Runs `f` on the input device, made on first use.
    #[cfg(windows)]
    fn with_input(
        &self,
        f: impl FnOnce(&mut enigo::Enigo) -> enigo::InputResult<()>,
    ) -> Result<(), String> {
        let mut guard = self.input.lock().unwrap_or_else(|e| e.into_inner());
        if guard.is_none() {
            let settings = enigo::Settings {
                // Held keys come up if Oxide stops in the middle.
                release_keys_when_dropped: true,
                ..Default::default()
            };
            *guard = Some(
                enigo::Enigo::new(&settings).map_err(|e| format!("input isn't available: {e}"))?,
            );
        }
        match guard.as_mut() {
            Some(e) => f(e).map_err(|e| format!("the input failed: {e}")),
            None => Err("input isn't available".into()),
        }
    }
}

#[cfg(windows)]
fn direction(p: Press) -> enigo::Direction {
    match p {
        Press::Down => enigo::Direction::Press,
        Press::Up => enigo::Direction::Release,
        Press::Click => enigo::Direction::Click,
    }
}

#[cfg(windows)]
fn enigo_key(k: KeyName) -> enigo::Key {
    use enigo::Key;
    match k {
        KeyName::Control => Key::Control,
        KeyName::Alt => Key::Alt,
        KeyName::Shift => Key::Shift,
        KeyName::Meta => Key::Meta,
        KeyName::Return => Key::Return,
        KeyName::Tab => Key::Tab,
        KeyName::Escape => Key::Escape,
        KeyName::Backspace => Key::Backspace,
        KeyName::Delete => Key::Delete,
        KeyName::Insert => Key::Insert,
        KeyName::Home => Key::Home,
        KeyName::End => Key::End,
        KeyName::PageUp => Key::PageUp,
        KeyName::PageDown => Key::PageDown,
        KeyName::Up => Key::UpArrow,
        KeyName::Down => Key::DownArrow,
        KeyName::Left => Key::LeftArrow,
        KeyName::Right => Key::RightArrow,
        KeyName::Space => Key::Space,
        KeyName::CapsLock => Key::CapsLock,
        KeyName::PrintScreen => Key::PrintScr,
        KeyName::F(n) => match n {
            1 => Key::F1,
            2 => Key::F2,
            3 => Key::F3,
            4 => Key::F4,
            5 => Key::F5,
            6 => Key::F6,
            7 => Key::F7,
            8 => Key::F8,
            9 => Key::F9,
            10 => Key::F10,
            11 => Key::F11,
            12 => Key::F12,
            13 => Key::F13,
            14 => Key::F14,
            15 => Key::F15,
            16 => Key::F16,
            17 => Key::F17,
            18 => Key::F18,
            19 => Key::F19,
            _ => Key::F20,
        },
        // Letters and digits as their virtual keys (VK_A = 'A', VK_0 = '0'), so Shift is
        // only pressed when the combo says so ("ctrl+shift+s"). Other characters go through
        // the keyboard layout.
        KeyName::Char(c) if c.is_ascii_alphanumeric() => {
            Key::Other(u32::from(c.to_ascii_uppercase()))
        }
        KeyName::Char(c) => Key::Unicode(c),
    }
}

/// Win32 and UI Automation calls.
///
/// `unsafe` is allowed here: the Win32 API is C. Each call gets plain values or buffers this
/// module owns, and handles are closed before returning.
#[cfg(windows)]
#[allow(unsafe_code)]
mod win {
    use std::ffi::c_void;
    use std::sync::mpsc;
    use std::time::Duration;

    use oxide_core::computer::{AppIdentity, FieldInfo};
    use windows::Win32::Foundation::{CloseHandle, HWND, POINT};
    use windows::Win32::Storage::FileSystem::{
        GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
    };
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
        QueryFullProcessImageNameW,
    };
    use windows::Win32::UI::HiDpi::{
        DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GA_ROOT, GetAncestor, GetCursorPos, GetForegroundWindow, GetWindowTextW,
        GetWindowThreadProcessId, SetCursorPos, WindowFromPoint,
    };
    use windows::core::{HSTRING, PCWSTR, PWSTR};

    /// Makes the process per-monitor DPI aware once (fails harmlessly when it already is).
    pub fn dpi_aware() {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| {
            let _ = unsafe {
                SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)
            };
        });
    }

    /// How long UI Automation may take to say what has focus.
    const UIA_TIMEOUT: Duration = Duration::from_millis(1500);

    pub fn move_to(x: i32, y: i32) -> Result<(), String> {
        unsafe { SetCursorPos(x, y) }.map_err(|e| format!("moving the mouse failed: {e}"))
    }

    pub fn cursor() -> Result<(i32, i32), String> {
        let mut p = POINT::default();
        unsafe { GetCursorPos(&mut p) }.map_err(|e| format!("reading the mouse failed: {e}"))?;
        Ok((p.x, p.y))
    }

    pub fn app_at(x: i32, y: i32) -> Option<AppIdentity> {
        let hwnd = unsafe { WindowFromPoint(POINT { x, y }) };
        if hwnd.0.is_null() {
            return None;
        }
        let root = unsafe { GetAncestor(hwnd, GA_ROOT) };
        identify(if root.0.is_null() { hwnd } else { root })
    }

    pub fn foreground_app() -> Option<AppIdentity> {
        let hwnd = unsafe { GetForegroundWindow() };
        if hwnd.0.is_null() {
            return None;
        }
        identify(hwnd)
    }

    fn identify(hwnd: HWND) -> Option<AppIdentity> {
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        if pid == 0 {
            return None;
        }
        let mut title = vec![0u16; 512];
        let n = unsafe { GetWindowTextW(hwnd, &mut title) };
        let title = String::from_utf16_lossy(&title[..usize::try_from(n).unwrap_or(0)]);
        let path = image_path(pid).unwrap_or_default();
        let name = if path.is_empty() {
            String::new()
        } else {
            file_description(&path).unwrap_or_default()
        };
        Some(AppIdentity {
            exe: path,
            name,
            title,
            pid,
        })
    }

    /// The program's full path.
    fn image_path(pid: u32) -> Option<String> {
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
        let mut buf = vec![0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = unsafe {
            QueryFullProcessImageNameW(
                handle,
                PROCESS_NAME_WIN32,
                PWSTR(buf.as_mut_ptr()),
                &mut len,
            )
        };
        let _ = unsafe { CloseHandle(handle) };
        ok.ok()?;
        Some(String::from_utf16_lossy(&buf[..len as usize]))
    }

    /// The program's own description ("Notepad").
    fn file_description(path: &str) -> Option<String> {
        let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
        let file = PCWSTR::from_raw(wide.as_ptr());
        let size = unsafe { GetFileVersionInfoSizeW(file, None) };
        if size == 0 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        unsafe { GetFileVersionInfoW(file, None, size, data.as_mut_ptr().cast()) }.ok()?;

        let mut ptr: *mut c_void = std::ptr::null_mut();
        let mut len = 0u32;
        let found = unsafe {
            VerQueryValueW(
                data.as_ptr().cast(),
                &HSTRING::from("\\VarFileInfo\\Translation"),
                &mut ptr,
                &mut len,
            )
        };
        if !found.as_bool() || ptr.is_null() || len < 4 {
            return None;
        }
        // Pairs of (language, code page), each two u16.
        let pairs = unsafe { std::slice::from_raw_parts(ptr.cast::<u16>(), len as usize / 2) };
        for pair in pairs.as_chunks::<2>().0 {
            let key = HSTRING::from(format!(
                "\\StringFileInfo\\{:04x}{:04x}\\FileDescription",
                pair[0], pair[1]
            ));
            let mut v: *mut c_void = std::ptr::null_mut();
            let mut vlen = 0u32;
            let ok = unsafe { VerQueryValueW(data.as_ptr().cast(), &key, &mut v, &mut vlen) };
            if !ok.as_bool() || v.is_null() || vlen == 0 {
                continue;
            }
            // `vlen` counts characters, including the final NUL.
            let s = unsafe { std::slice::from_raw_parts(v.cast::<u16>(), vlen as usize) };
            let text = String::from_utf16_lossy(s);
            let text = text.trim_end_matches('\0').trim();
            if !text.is_empty() {
                return Some(text.to_owned());
            }
        }
        None
    }

    /// Asks UI Automation on its own thread (COM), giving up after [`UIA_TIMEOUT`].
    pub fn focused_field() -> Option<FieldInfo> {
        let (tx, rx) = mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("oxide-uia".into())
            .spawn(move || {
                let _ = tx.send(read_focused());
            });
        if spawned.is_err() {
            return None;
        }
        match rx.recv_timeout(UIA_TIMEOUT) {
            Ok(v) => v,
            Err(_) => {
                tracing::warn!("UI Automation didn't say which control has focus in time");
                None
            }
        }
    }

    fn read_focused() -> Option<FieldInfo> {
        let uia = uiautomation::UIAutomation::new().ok()?;
        let el = uia.get_focused_element().ok()?;
        Some(FieldInfo {
            is_password: el.is_password().unwrap_or(false),
            name: el.get_name().unwrap_or_default(),
            automation_id: el.get_automation_id().unwrap_or_default(),
            class_name: el.get_classname().unwrap_or_default(),
            help_text: el.get_help_text().unwrap_or_default(),
        })
    }
}
