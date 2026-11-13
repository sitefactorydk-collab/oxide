//! Computer use (Phase 16): models see the screen and use the mouse and keyboard.
//!
//! Plain data and pure helpers only: the actions a model asks for and how they are read,
//! key names, how screenshots are scaled and coordinates mapped back, and the safety rules
//! (apps never used, fields never typed into, text never typed). Capturing and input are in
//! `oxide-computer`; the executor and approvals are in `oxide-agent`.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use specta::Type;

/// The tool's name (one tool with an `action`, like Anthropic's `computer_20251124`).
pub const COMPUTER_TOOL: &str = "computer";
/// `ChatTools::disabled_builtins` entry that switches computer use off in a chat.
pub const COMPUTER_SWITCH: &str = "computer";
/// Grant key of "Allow for this chat" on the card that starts computer use in a chat.
pub const COMPUTER_SESSION_GRANT: &str = "builtin/computer";
/// Prefix of the grant keys of apps allowed in a chat (`computer-app/<app id>`).
pub const COMPUTER_APP_GRANT: &str = "computer-app/";
/// Stops every chat that controls the computer, from anywhere.
pub const PANIC_SHORTCUT: &str = "Ctrl+Alt+Escape";
/// What the panic shortcut is called in text for users.
pub const PANIC_SHORTCUT_LABEL: &str = "Ctrl+Alt+Esc";
/// The result of computer calls after one that failed in the same turn (Anthropic's wording).
pub const HALT_TEXT: &str = "Not executed: an earlier computer action in this turn failed.";

/// Longest edge, height and pixel count of the screenshots models get. Anthropic recommends
/// about 1280×800 for accuracy and at most ~1.15 megapixels for older models.
pub const MAX_SHOT_WIDTH: u32 = 1366;
pub const MAX_SHOT_HEIGHT: u32 = 1024;
pub const MAX_SHOT_PIXELS: u64 = 1_150_000;
/// Longest wait or key hold, in seconds.
pub const MAX_WAIT_SECS: f64 = 300.0;
/// Most characters one `type` action may type.
pub const MAX_TYPE_CHARS: usize = 5_000;
/// Screenshots kept in a request (older ones become a short note).
pub const SCREENSHOTS_KEPT: usize = 3;

/// The actions, as Anthropic's computer toolset names its members.
pub const COMPUTER_ACTIONS: [&str; 17] = [
    "screenshot",
    "zoom",
    "left_click",
    "right_click",
    "middle_click",
    "double_click",
    "triple_click",
    "left_click_drag",
    "mouse_move",
    "left_mouse_down",
    "left_mouse_up",
    "cursor_position",
    "scroll",
    "type",
    "key",
    "hold_key",
    "wait",
];

// ---- Settings, displays, apps ----------------------------------------------------------------

/// Settings → Computer use. Off by default: while on, models in a chat may ask to control
/// the computer (each chat asks first, and each app it uses asks too).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct ComputerSettings {
    pub enabled: bool,
    /// Which screen models see and use ([`ComputerDisplay::id`]); `None`: the main one.
    pub display: Option<String>,
    /// Minimize Oxide's window while a chat acts, so models see what's behind it.
    pub hide_oxide: bool,
    /// On Claude, use Claude's own computer tool (else the same function tool as other
    /// models).
    pub native_anthropic: bool,
    /// Apps every chat may use without asking.
    pub always_allowed_apps: Vec<ComputerApp>,
}

impl Default for ComputerSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            display: None,
            hide_oxide: true,
            native_anthropic: true,
            always_allowed_apps: Vec::new(),
        }
    }
}

impl ComputerSettings {
    pub fn normalized(mut self) -> Self {
        self.display = self
            .display
            .map(|d| d.trim().to_owned())
            .filter(|d| !d.is_empty());
        let mut apps: Vec<ComputerApp> = Vec::new();
        for a in self.always_allowed_apps {
            let id = a.id.trim().to_lowercase();
            if id.is_empty() || apps.iter().any(|x| x.id == id) {
                continue;
            }
            let name = a.name.trim().chars().take(100).collect::<String>();
            apps.push(ComputerApp {
                name: if name.is_empty() { id.clone() } else { name },
                id,
            });
        }
        apps.truncate(200);
        self.always_allowed_apps = apps;
        self
    }

    /// Whether an app may be used in every chat.
    pub fn always_allows(&self, app: &ComputerApp) -> bool {
        self.always_allowed_apps.iter().any(|a| a.id == app.id)
    }
}

/// A screen, in the coordinates the input functions use (physical pixels on Windows).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ComputerDisplay {
    /// Stable id (the system's name for the screen).
    pub id: String,
    /// Name for users ("Dell U2720Q").
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f32,
    pub primary: bool,
}

impl ComputerDisplay {
    /// "Dell U2720Q (2560×1440)".
    pub fn label(&self) -> String {
        format!("{} ({}×{})", self.name, self.width, self.height)
    }

    pub fn contains(&self, x: i32, y: i32) -> bool {
        let (w, h) = (i64::from(self.width), i64::from(self.height));
        let (dx, dy) = (
            i64::from(x) - i64::from(self.x),
            i64::from(y) - i64::from(self.y),
        );
        dx >= 0 && dy >= 0 && dx < w && dy < h
    }
}

/// The screen a chat uses: the one in settings, else the main one, else the first.
pub fn pick_display<'a>(
    displays: &'a [ComputerDisplay],
    wanted: Option<&str>,
) -> Option<&'a ComputerDisplay> {
    wanted
        .and_then(|w| displays.iter().find(|d| d.id == w))
        .or_else(|| displays.iter().find(|d| d.primary))
        .or_else(|| displays.first())
}

/// An app, as approvals and settings name it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ComputerApp {
    /// Stable id: the program's file name in lower case (`notepad.exe`), or a group id
    /// (`windows-shell`).
    pub id: String,
    /// Name for users ("Notepad").
    pub name: String,
}

/// What the system says about the program behind a window.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AppIdentity {
    /// The program's file name (`notepad.exe`), any case.
    pub exe: String,
    /// Its own description ("Notepad"); may be empty.
    pub name: String,
    /// The window's title; may be empty.
    pub title: String,
    pub pid: u32,
}

/// The parts of Windows (and macOS) people think of as "the computer", not an app: one
/// approval covers them all.
const SHELL_PROGRAMS: [&str; 11] = [
    "explorer.exe",
    "startmenuexperiencehost.exe",
    "searchhost.exe",
    "searchapp.exe",
    "searchui.exe",
    "shellexperiencehost.exe",
    "shellhost.exe",
    "textinputhost.exe",
    "lockapp.exe",
    "dock",
    "finder",
];
pub const SHELL_APP_ID: &str = "windows-shell";

/// Programs that host other apps' windows: the window's title names the app.
const HOST_PROGRAMS: [&str; 1] = ["applicationframehost.exe"];

/// The app behind a window, as approvals name it.
pub fn app_of(id: &AppIdentity) -> ComputerApp {
    let exe = id.exe.trim().to_lowercase();
    let file = exe.rsplit(['\\', '/']).next().unwrap_or(&exe).to_owned();
    if SHELL_PROGRAMS.contains(&file.as_str()) {
        return ComputerApp {
            id: SHELL_APP_ID.into(),
            name: if cfg!(target_os = "macos") {
                "Finder and the Dock".into()
            } else {
                "Windows (taskbar, Start and search)".into()
            },
        };
    }
    if HOST_PROGRAMS.contains(&file.as_str()) && !id.title.trim().is_empty() {
        let title: String = id.title.trim().chars().take(80).collect();
        return ComputerApp {
            id: format!("app:{}", title.to_lowercase()),
            name: title,
        };
    }
    let name = if id.name.trim().is_empty() {
        file.trim_end_matches(".exe").to_owned()
    } else {
        id.name.trim().chars().take(80).collect()
    };
    ComputerApp {
        id: if file.is_empty() {
            format!("app:{}", name.to_lowercase())
        } else {
            file
        },
        name,
    }
}

/// Programs models never use: password managers and the system's sign-in and credential
/// prompts. Returns why.
pub fn blocked_app(id: &AppIdentity, own_pid: u32) -> Option<String> {
    if id.pid != 0 && id.pid == own_pid {
        return Some("That is Oxide's own window: computer use never acts on Oxide itself.".into());
    }
    let exe = id.exe.trim().to_lowercase();
    let file = exe.rsplit(['\\', '/']).next().unwrap_or(&exe);
    const BLOCKED: [&str; 16] = [
        "1password.exe",
        "bitwarden.exe",
        "keepass.exe",
        "keepassxc.exe",
        "lastpass.exe",
        "dashlane.exe",
        "enpass.exe",
        "nordpass.exe",
        "proton pass.exe",
        "roboform.exe",
        "keeper.exe",
        "credentialuibroker.exe",
        "consent.exe",
        "logonui.exe",
        "keychain access",
        "securityagent",
    ];
    let name = id.name.to_lowercase();
    if BLOCKED.contains(&file) || name.contains("password manager") || name == "passwords" {
        let shown = if id.name.trim().is_empty() {
            file.to_owned()
        } else {
            id.name.trim().to_owned()
        };
        return Some(format!(
            "{shown} holds passwords or sign-ins, so computer use never uses it. Ask the user to \
             do this part."
        ));
    }
    None
}

// ---- Fields and text never typed ------------------------------------------------------------

/// What UI Automation (or the platform's equivalent) says about the focused control.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FieldInfo {
    pub is_password: bool,
    pub name: String,
    pub automation_id: String,
    pub class_name: String,
    pub help_text: String,
}

/// Why typing into this field is refused (passwords, one-time codes, payment and bank
/// details), or `None`.
pub fn sensitive_field(f: &FieldInfo) -> Option<String> {
    if f.is_password {
        return Some("a password field".into());
    }
    let text = format!(
        "{} {} {} {}",
        f.name, f.automation_id, f.class_name, f.help_text
    )
    .to_lowercase();
    const PHRASES: [(&str, &str); 24] = [
        ("password", "a password field"),
        ("passwort", "a password field"),
        ("passcode", "a password field"),
        ("adgangskode", "a password field"),
        ("kodeord", "a password field"),
        ("one-time code", "a one-time code field"),
        ("one time code", "a one-time code field"),
        ("verification code", "a one-time code field"),
        ("security code", "a card's security code"),
        ("card number", "a payment card field"),
        ("cardnumber", "a payment card field"),
        ("card-number", "a payment card field"),
        ("cc-number", "a payment card field"),
        ("cc-csc", "a card's security code"),
        ("cc-exp", "a payment card field"),
        ("kortnummer", "a payment card field"),
        ("credit card", "a payment card field"),
        ("debit card", "a payment card field"),
        ("expiration date", "a payment card field"),
        ("iban", "a bank account field"),
        ("account number", "a bank account field"),
        ("kontonummer", "a bank account field"),
        ("routing number", "a bank account field"),
        ("sort code", "a bank account field"),
    ];
    for (p, why) in PHRASES {
        if text.contains(p) {
            return Some(why.into());
        }
    }
    // Short words only as whole words ("pin" not in "spinner").
    let words: Vec<&str> = text
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    for (w, why) in [
        ("pin", "a PIN field"),
        ("otp", "a one-time code field"),
        ("cvv", "a card's security code"),
        ("cvc", "a card's security code"),
        ("cvv2", "a card's security code"),
    ] {
        if words.contains(&w) {
            return Some(why.into());
        }
    }
    None
}

/// Why this text is never typed (it holds a payment card or bank account number), or
/// `None`.
pub fn sensitive_text(text: &str) -> Option<String> {
    // Card numbers: 13–19 digits, maybe split by spaces or dashes, passing Luhn's check.
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_ascii_digit() && (i == 0 || !chars[i - 1].is_ascii_alphanumeric()) {
            let mut digits = String::new();
            let mut j = i;
            while j < chars.len() {
                let c = chars[j];
                if c.is_ascii_digit() {
                    digits.push(c);
                } else if (c == ' ' || c == '-')
                    && j + 1 < chars.len()
                    && chars[j + 1].is_ascii_digit()
                {
                } else {
                    break;
                }
                j += 1;
            }
            if (13..=19).contains(&digits.len()) && luhn(&digits) {
                return Some("a payment card number".into());
            }
            i = j.max(i + 1);
        } else {
            i += 1;
        }
    }
    // IBANs: two letters, two digits, 11–30 more letters or digits (spaces allowed), with
    // a valid check.
    let compact: Vec<char> = text.chars().filter(|c| *c != ' ').collect();
    for start in 0..compact.len() {
        if start > 0 && compact[start - 1].is_ascii_alphanumeric() {
            continue;
        }
        let rest = &compact[start..];
        if rest.len() < 15
            || !rest[0].is_ascii_uppercase()
            || !rest[1].is_ascii_uppercase()
            || !rest[2].is_ascii_digit()
            || !rest[3].is_ascii_digit()
        {
            continue;
        }
        let len = rest
            .iter()
            .take_while(|c| c.is_ascii_digit() || c.is_ascii_uppercase())
            .count()
            .min(34);
        for l in (15..=len).rev() {
            if iban_ok(&rest[..l]) {
                return Some("a bank account number (IBAN)".into());
            }
        }
    }
    None
}

fn luhn(digits: &str) -> bool {
    let mut sum = 0u32;
    for (i, c) in digits.chars().rev().enumerate() {
        let Some(mut d) = c.to_digit(10) else {
            return false;
        };
        if i % 2 == 1 {
            d *= 2;
            if d > 9 {
                d -= 9;
            }
        }
        sum += d;
    }
    sum.is_multiple_of(10)
}

fn iban_ok(s: &[char]) -> bool {
    if s.len() < 15 || s.len() > 34 {
        return false;
    }
    let mut rearranged: Vec<char> = s[4..].to_vec();
    rearranged.extend_from_slice(&s[..4]);
    let mut rem: u32 = 0;
    for c in rearranged {
        let v = if c.is_ascii_digit() {
            c as u32 - '0' as u32
        } else if c.is_ascii_uppercase() {
            c as u32 - 'A' as u32 + 10
        } else {
            return false;
        };
        rem = if v >= 10 {
            (rem * 100 + v) % 97
        } else {
            (rem * 10 + v) % 97
        };
    }
    rem == 1
}

// ---- Keys -------------------------------------------------------------------------------------

/// A key, named the way models write them (`ctrl`, `Return`, `Page_Down`, `F5`, `a`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyName {
    Control,
    Alt,
    Shift,
    /// Windows key / Command.
    Meta,
    Return,
    Tab,
    Escape,
    Backspace,
    Delete,
    Insert,
    Home,
    End,
    PageUp,
    PageDown,
    Up,
    Down,
    Left,
    Right,
    Space,
    CapsLock,
    PrintScreen,
    /// F1–F20.
    F(u8),
    Char(char),
}

impl KeyName {
    pub fn is_modifier(self) -> bool {
        matches!(
            self,
            KeyName::Control | KeyName::Alt | KeyName::Shift | KeyName::Meta
        )
    }

    /// Whether pressing it alone (or with Shift) enters a character.
    pub fn enters_text(self) -> bool {
        matches!(self, KeyName::Char(_) | KeyName::Space)
    }
}

fn key_named(word: &str) -> Option<KeyName> {
    let w = word.trim();
    if w.chars().count() == 1 {
        let c = w.chars().next()?;
        return Some(if c == ' ' {
            KeyName::Space
        } else {
            KeyName::Char(c)
        });
    }
    let l = w.to_lowercase().replace(['-', ' '], "_");
    let k = match l.as_str() {
        "ctrl" | "control" | "ctl" | "lctrl" | "rctrl" | "control_l" | "control_r" => {
            KeyName::Control
        }
        "alt" | "option" | "opt" | "lalt" | "ralt" | "alt_l" | "alt_r" | "altgr" => KeyName::Alt,
        "shift" | "lshift" | "rshift" | "shift_l" | "shift_r" => KeyName::Shift,
        "super" | "super_l" | "super_r" | "win" | "windows" | "meta" | "cmd" | "command"
        | "logo" | "os" => KeyName::Meta,
        "return" | "enter" | "kp_enter" => KeyName::Return,
        "tab" => KeyName::Tab,
        "escape" | "esc" => KeyName::Escape,
        "backspace" | "back_space" | "bksp" => KeyName::Backspace,
        "delete" | "del" => KeyName::Delete,
        "insert" | "ins" => KeyName::Insert,
        "home" => KeyName::Home,
        "end" => KeyName::End,
        "page_up" | "pageup" | "pgup" | "prior" => KeyName::PageUp,
        "page_down" | "pagedown" | "pgdn" | "next" => KeyName::PageDown,
        "up" | "arrowup" | "up_arrow" | "arrow_up" => KeyName::Up,
        "down" | "arrowdown" | "down_arrow" | "arrow_down" => KeyName::Down,
        "left" | "arrowleft" | "left_arrow" | "arrow_left" => KeyName::Left,
        "right" | "arrowright" | "right_arrow" | "arrow_right" => KeyName::Right,
        "space" | "spacebar" => KeyName::Space,
        "capslock" | "caps_lock" | "caps" => KeyName::CapsLock,
        "print" | "printscreen" | "print_screen" | "prtsc" | "prtscn" | "sysrq" => {
            KeyName::PrintScreen
        }
        "plus" => KeyName::Char('+'),
        "minus" => KeyName::Char('-'),
        "comma" => KeyName::Char(','),
        "period" | "dot" => KeyName::Char('.'),
        "slash" => KeyName::Char('/'),
        "backslash" => KeyName::Char('\\'),
        "semicolon" => KeyName::Char(';'),
        "apostrophe" | "quote" => KeyName::Char('\''),
        "grave" | "backtick" => KeyName::Char('`'),
        "bracketleft" => KeyName::Char('['),
        "bracketright" => KeyName::Char(']'),
        "equal" | "equals" => KeyName::Char('='),
        _ => {
            if let Some(n) = l.strip_prefix('f').and_then(|n| n.parse::<u8>().ok())
                && (1..=20).contains(&n)
            {
                KeyName::F(n)
            } else {
                return None;
            }
        }
    };
    Some(k)
}

/// One combination: `ctrl+shift+s` → [Control, Shift, Char('s')]. Letters with modifiers
/// are lower case (Shift says itself).
pub fn parse_combo(text: &str) -> Result<Vec<KeyName>, String> {
    let t = text.trim();
    if t.is_empty() {
        return Err("No key given.".into());
    }
    // "ctrl++" and "+" mean the plus key.
    let mut parts: Vec<String> = Vec::new();
    let mut cur = String::new();
    let chars: Vec<char> = t.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        if *c == '+' && !cur.is_empty() {
            parts.push(std::mem::take(&mut cur));
        } else if *c == '+' && cur.is_empty() && (i + 1 == chars.len()) {
            cur.push('+');
        } else if *c == '+' && cur.is_empty() {
            // "++x": a plus key in the middle.
            cur.push('+');
            parts.push(std::mem::take(&mut cur));
        } else {
            cur.push(*c);
        }
    }
    if !cur.is_empty() {
        parts.push(cur);
    }
    let mut keys = Vec::new();
    for p in &parts {
        let k = key_named(p).ok_or_else(|| {
            format!(
                "“{p}” isn't a key Oxide knows. Use names like ctrl, alt, shift, super, Return, \
                 Tab, Escape, BackSpace, Delete, Home, End, Page_Up, Page_Down, Up, Down, Left, \
                 Right, space, F1–F20, or a single character."
            )
        })?;
        keys.push(k);
    }
    if keys.len() > 1 {
        for k in &mut keys {
            if let KeyName::Char(c) = k
                && c.is_ascii_uppercase()
            {
                *k = KeyName::Char(c.to_ascii_lowercase());
            }
        }
    }
    if keys.len() > 6 {
        return Err("That's too many keys at once.".into());
    }
    Ok(keys)
}

/// Combinations pressed one after another: `"ctrl+a Delete"` (xdotool style).
pub fn parse_keys(text: &str) -> Result<Vec<Vec<KeyName>>, String> {
    if !text.is_empty() && text.trim().is_empty() {
        return Ok(vec![vec![KeyName::Space]]);
    }
    let t = text.trim();
    if t.chars().count() == 1 {
        return Ok(vec![parse_combo(t)?]);
    }
    let combos: Vec<Vec<KeyName>> = t
        .split_whitespace()
        .map(parse_combo)
        .collect::<Result<_, _>>()?;
    if combos.is_empty() {
        return Err("No key given.".into());
    }
    if combos.len() > 20 {
        return Err("That's too many keys in one action; use type for text.".into());
    }
    Ok(combos)
}

/// Modifier keys held during a click or scroll (`"ctrl"`, `"ctrl+shift"`).
pub fn parse_modifiers(text: Option<&str>) -> Result<Vec<KeyName>, String> {
    let Some(t) = text.map(str::trim).filter(|t| !t.is_empty()) else {
        return Ok(Vec::new());
    };
    let keys = parse_combo(t)?;
    if keys.iter().any(|k| !k.is_modifier()) {
        return Err(format!(
            "“{t}” should be modifier keys to hold (shift, ctrl, alt, super, joined by +)."
        ));
    }
    Ok(keys)
}

// ---- Actions ----------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollDirection {
    Up,
    Down,
    Left,
    Right,
}

/// One action, read from a call's input. Coordinates are in screenshot pixels.
#[derive(Debug, Clone, PartialEq)]
pub enum ComputerAction {
    Screenshot,
    /// A part of the screen at full detail: `[x0, y0, x1, y1]`.
    Zoom {
        region: [i32; 4],
    },
    Click {
        button: MouseButton,
        count: u8,
        at: Option<(i32, i32)>,
        modifiers: Vec<KeyName>,
    },
    Drag {
        from: (i32, i32),
        to: (i32, i32),
        modifiers: Vec<KeyName>,
    },
    MouseMove {
        to: (i32, i32),
    },
    MouseDown,
    MouseUp,
    CursorPosition,
    Scroll {
        direction: ScrollDirection,
        amount: u32,
        at: Option<(i32, i32)>,
        modifiers: Vec<KeyName>,
    },
    Type {
        text: String,
    },
    Key {
        keys: Vec<Vec<KeyName>>,
        /// The original text, for labels.
        text: String,
        repeat: u32,
    },
    HoldKey {
        keys: Vec<KeyName>,
        text: String,
        duration_ms: u64,
    },
    Wait {
        duration_ms: u64,
    },
}

impl ComputerAction {
    /// The action's name ([`COMPUTER_ACTIONS`]).
    pub fn name(&self) -> &'static str {
        match self {
            ComputerAction::Screenshot => "screenshot",
            ComputerAction::Zoom { .. } => "zoom",
            ComputerAction::Click {
                button: MouseButton::Left,
                count: 2,
                ..
            } => "double_click",
            ComputerAction::Click {
                button: MouseButton::Left,
                count: 3,
                ..
            } => "triple_click",
            ComputerAction::Click {
                button: MouseButton::Left,
                ..
            } => "left_click",
            ComputerAction::Click {
                button: MouseButton::Right,
                ..
            } => "right_click",
            ComputerAction::Click {
                button: MouseButton::Middle,
                ..
            } => "middle_click",
            ComputerAction::Drag { .. } => "left_click_drag",
            ComputerAction::MouseMove { .. } => "mouse_move",
            ComputerAction::MouseDown => "left_mouse_down",
            ComputerAction::MouseUp => "left_mouse_up",
            ComputerAction::CursorPosition => "cursor_position",
            ComputerAction::Scroll { .. } => "scroll",
            ComputerAction::Type { .. } => "type",
            ComputerAction::Key { .. } => "key",
            ComputerAction::HoldKey { .. } => "hold_key",
            ComputerAction::Wait { .. } => "wait",
        }
    }

    /// Only looks: nothing on the computer changes (no app approval needed).
    pub fn only_looks(&self) -> bool {
        matches!(
            self,
            ComputerAction::Screenshot
                | ComputerAction::Zoom { .. }
                | ComputerAction::CursorPosition
                | ComputerAction::Wait { .. }
                | ComputerAction::MouseMove { .. }
        )
    }

    /// Changes something, so a screenshot afterwards shows what happened.
    pub fn shows_result(&self) -> bool {
        !self.only_looks() && !matches!(self, ComputerAction::MouseDown)
    }

    /// For the tool line and the control bar: "Clicking at (512, 300)".
    pub fn label(&self) -> String {
        let at = |p: &Option<(i32, i32)>| match p {
            Some((x, y)) => format!(" at ({x}, {y})"),
            None => String::new(),
        };
        match self {
            ComputerAction::Screenshot => "Taking a screenshot".into(),
            ComputerAction::Zoom { .. } => "Zooming in".into(),
            ComputerAction::Click {
                button,
                count,
                at: p,
                ..
            } => {
                let what = match (button, count) {
                    (MouseButton::Left, 2) => "Double-clicking",
                    (MouseButton::Left, 3) => "Triple-clicking",
                    (MouseButton::Left, _) => "Clicking",
                    (MouseButton::Right, _) => "Right-clicking",
                    (MouseButton::Middle, _) => "Middle-clicking",
                };
                format!("{what}{}", at(p))
            }
            ComputerAction::Drag { from, to, .. } => format!(
                "Dragging from ({}, {}) to ({}, {})",
                from.0, from.1, to.0, to.1
            ),
            ComputerAction::MouseMove { to } => format!("Moving the mouse to ({}, {})", to.0, to.1),
            ComputerAction::MouseDown => "Pressing the mouse button".into(),
            ComputerAction::MouseUp => "Releasing the mouse button".into(),
            ComputerAction::CursorPosition => "Finding the mouse pointer".into(),
            ComputerAction::Scroll {
                direction, at: p, ..
            } => {
                let d = match direction {
                    ScrollDirection::Up => "up",
                    ScrollDirection::Down => "down",
                    ScrollDirection::Left => "left",
                    ScrollDirection::Right => "right",
                };
                format!("Scrolling {d}{}", at(p))
            }
            ComputerAction::Type { text } => {
                let short: String = text.chars().take(40).collect();
                let more = if text.chars().count() > 40 { "…" } else { "" };
                format!("Typing “{}{more}”", short.replace('\n', " "))
            }
            ComputerAction::Key { text, repeat, .. } => {
                if *repeat > 1 {
                    format!("Pressing {text} ×{repeat}")
                } else {
                    format!("Pressing {text}")
                }
            }
            ComputerAction::HoldKey { text, .. } => format!("Holding {text}"),
            ComputerAction::Wait { duration_ms } => {
                format!("Waiting {}", seconds_label(*duration_ms))
            }
        }
    }
}

fn seconds_label(ms: u64) -> String {
    if ms.is_multiple_of(1000) {
        format!("{} s", ms / 1000)
    } else {
        format!("{:.1} s", ms as f64 / 1000.0)
    }
}

fn point(v: Option<&Value>, what: &str) -> Result<Option<(i32, i32)>, String> {
    let Some(v) = v.filter(|v| !v.is_null()) else {
        return Ok(None);
    };
    let pair = match v {
        Value::Array(a) if a.len() == 2 => (a[0].as_f64(), a[1].as_f64()),
        Value::Object(o) => (
            o.get("x").and_then(Value::as_f64),
            o.get("y").and_then(Value::as_f64),
        ),
        Value::String(s) => {
            let mut it = s
                .trim_matches(|c| c == '[' || c == ']' || c == '(' || c == ')')
                .split(',')
                .map(|p| p.trim().parse::<f64>().ok());
            (it.next().flatten(), it.next().flatten())
        }
        _ => (None, None),
    };
    match pair {
        (Some(x), Some(y)) if x.is_finite() && y.is_finite() => {
            if x < 0.0 || y < 0.0 {
                return Err(format!(
                    "{what} can't be negative: use pixels of the screenshot."
                ));
            }
            Ok(Some((x.round() as i32, y.round() as i32)))
        }
        _ => Err(format!("{what} must be [x, y] in screenshot pixels.")),
    }
}

fn seconds(v: Option<&Value>, what: &str, default: Option<f64>) -> Result<u64, String> {
    let s = match v.and_then(|v| v.as_f64().or_else(|| v.as_str()?.trim().parse().ok())) {
        Some(s) => s,
        None => default.ok_or_else(|| format!("{what} needs a duration in seconds."))?,
    };
    if !s.is_finite() || !(0.0..=MAX_WAIT_SECS).contains(&s) {
        return Err(format!(
            "{what}'s duration must be 0–{MAX_WAIT_SECS} seconds."
        ));
    }
    Ok((s * 1000.0).round() as u64)
}

/// Reads a call's input: `{"action": "left_click", "coordinate": [512, 300]}`.
pub fn parse_action(input: &Value) -> Result<ComputerAction, String> {
    let action = input
        .get("action")
        .and_then(Value::as_str)
        .map(str::trim)
        .ok_or("The call needs an \"action\" (screenshot, left_click, type, key, …).")?;
    let text = input.get("text").and_then(Value::as_str);
    let coordinate = point(input.get("coordinate"), "coordinate")?;
    let click = |button: MouseButton, count: u8| -> Result<ComputerAction, String> {
        Ok(ComputerAction::Click {
            button,
            count,
            at: coordinate,
            modifiers: parse_modifiers(text)?,
        })
    };
    match action {
        "screenshot" => Ok(ComputerAction::Screenshot),
        "zoom" => {
            let r = input
                .get("region")
                .and_then(Value::as_array)
                .filter(|a| a.len() == 4)
                .and_then(|a| {
                    let v: Vec<f64> = a.iter().filter_map(Value::as_f64).collect();
                    (v.len() == 4).then(|| v.iter().map(|n| n.round() as i32).collect::<Vec<_>>())
                })
                .ok_or("zoom needs a \"region\": [x0, y0, x1, y1] in screenshot pixels.")?;
            if r[0] < 0 || r[1] < 0 || r[2] <= r[0] || r[3] <= r[1] {
                return Err(
                    "zoom's region must be [x0, y0, x1, y1] with x1 > x0 and y1 > y0.".into(),
                );
            }
            Ok(ComputerAction::Zoom {
                region: [r[0], r[1], r[2], r[3]],
            })
        }
        "left_click" | "click" => click(MouseButton::Left, 1),
        "right_click" => click(MouseButton::Right, 1),
        "middle_click" => click(MouseButton::Middle, 1),
        "double_click" => click(MouseButton::Left, 2),
        "triple_click" => click(MouseButton::Left, 3),
        "left_click_drag" | "drag" => {
            let to = coordinate.ok_or("left_click_drag needs \"coordinate\" (where to drop).")?;
            let from = point(input.get("start_coordinate"), "start_coordinate")?
                .ok_or("left_click_drag needs \"start_coordinate\" (where to start).")?;
            Ok(ComputerAction::Drag {
                from,
                to,
                modifiers: parse_modifiers(text)?,
            })
        }
        "mouse_move" => Ok(ComputerAction::MouseMove {
            to: coordinate.ok_or("mouse_move needs \"coordinate\".")?,
        }),
        "left_mouse_down" => Ok(ComputerAction::MouseDown),
        "left_mouse_up" => Ok(ComputerAction::MouseUp),
        "cursor_position" => Ok(ComputerAction::CursorPosition),
        "scroll" => {
            let direction = match input
                .get("scroll_direction")
                .and_then(Value::as_str)
                .map(|s| s.trim().to_lowercase())
                .as_deref()
            {
                Some("up") => ScrollDirection::Up,
                Some("down") | None => ScrollDirection::Down,
                Some("left") => ScrollDirection::Left,
                Some("right") => ScrollDirection::Right,
                Some(other) => {
                    return Err(format!(
                        "scroll_direction “{other}” must be up, down, left or right."
                    ));
                }
            };
            let amount = input
                .get("scroll_amount")
                .and_then(Value::as_f64)
                .map(|a| a.round())
                .unwrap_or(3.0);
            if !(1.0..=50.0).contains(&amount) {
                return Err("scroll_amount must be 1–50 wheel clicks.".into());
            }
            Ok(ComputerAction::Scroll {
                direction,
                amount: amount as u32,
                at: coordinate,
                modifiers: parse_modifiers(text)?,
            })
        }
        "type" => {
            let t = text.ok_or("type needs \"text\".")?;
            if t.is_empty() {
                return Err("type needs some \"text\".".into());
            }
            if t.contains('\0') {
                return Err("The text can't contain a null character.".into());
            }
            if t.chars().count() > MAX_TYPE_CHARS {
                return Err(format!(
                    "That's more than {MAX_TYPE_CHARS} characters: type it in parts."
                ));
            }
            Ok(ComputerAction::Type { text: t.to_owned() })
        }
        "key" => {
            let t = text.ok_or("key needs \"text\": a key or combination, e.g. ctrl+s.")?;
            let repeat = input
                .get("repeat")
                .and_then(Value::as_f64)
                .map(|r| r.round())
                .unwrap_or(1.0);
            if !(1.0..=100.0).contains(&repeat) {
                return Err("repeat must be 1–100.".into());
            }
            Ok(ComputerAction::Key {
                keys: parse_keys(t)?,
                text: t.trim().to_owned(),
                repeat: repeat as u32,
            })
        }
        "hold_key" => {
            let t = text.ok_or("hold_key needs \"text\": the key or combination to hold.")?;
            Ok(ComputerAction::HoldKey {
                keys: parse_combo(t)?,
                text: t.trim().to_owned(),
                duration_ms: seconds(input.get("duration"), "hold_key", None)?,
            })
        }
        "wait" => Ok(ComputerAction::Wait {
            duration_ms: seconds(input.get("duration"), "wait", Some(1.0))?,
        }),
        other => Err(format!(
            "“{other}” isn't an action. Use one of: {}.",
            COMPUTER_ACTIONS.join(", ")
        )),
    }
}

/// The function tool's input schema (for models without Anthropic's own computer tool).
pub fn computer_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "action": {
                "type": "string",
                "enum": COMPUTER_ACTIONS,
                "description": "What to do. screenshot: see the screen. left_click / right_click / middle_click / double_click / triple_click: click at coordinate (or where the mouse is). left_click_drag: press at start_coordinate, release at coordinate. mouse_move: move the pointer. left_mouse_down / left_mouse_up: press or release the left button. cursor_position: where the pointer is. scroll: turn the wheel at coordinate. type: type text. key: press a key or combination (e.g. ctrl+s, Return). hold_key: hold keys for duration seconds. wait: pause for duration seconds. zoom: see region at full detail."
            },
            "coordinate": {
                "type": "array", "items": { "type": "number" }, "minItems": 2, "maxItems": 2,
                "description": "[x, y] in pixels of the latest screenshot, from its top-left corner."
            },
            "start_coordinate": {
                "type": "array", "items": { "type": "number" }, "minItems": 2, "maxItems": 2,
                "description": "left_click_drag: [x, y] where the drag starts."
            },
            "text": {
                "type": "string",
                "description": "type: the text. key / hold_key: the key or combination (ctrl+s, Return, alt+Tab, super). Clicks and scroll: modifier keys to hold (shift, ctrl, alt, super; joined by +)."
            },
            "scroll_direction": { "type": "string", "enum": ["up", "down", "left", "right"] },
            "scroll_amount": { "type": "integer", "minimum": 1, "maximum": 50, "description": "Wheel clicks (default 3)." },
            "duration": { "type": "number", "minimum": 0, "maximum": 300, "description": "wait / hold_key: seconds." },
            "repeat": { "type": "integer", "minimum": 1, "maximum": 100, "description": "key: how many times to press it." },
            "region": {
                "type": "array", "items": { "type": "number" }, "minItems": 4, "maxItems": 4,
                "description": "zoom: [x0, y0, x1, y1] in screenshot pixels."
            }
        },
        "required": ["action"]
    })
}

// ---- Geometry ---------------------------------------------------------------------------------

/// The size of the screenshots models get of a screen this big (never larger than it).
pub fn screenshot_size(width: u32, height: u32) -> (u32, u32) {
    if width == 0 || height == 0 {
        return (width.max(1), height.max(1));
    }
    let (w, h) = (f64::from(width), f64::from(height));
    let scale = 1.0_f64
        .min(f64::from(MAX_SHOT_WIDTH) / w)
        .min(f64::from(MAX_SHOT_HEIGHT) / h)
        .min((MAX_SHOT_PIXELS as f64 / (w * h)).sqrt());
    let sw = ((w * scale).round() as u32).clamp(1, width);
    let sh = ((h * scale).round() as u32).clamp(1, height);
    (sw, sh)
}

/// A screen and the size of its screenshots: maps screenshot pixels to the screen and back.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    pub display: ComputerDisplay,
    pub width: u32,
    pub height: u32,
}

impl Frame {
    pub fn new(display: ComputerDisplay) -> Self {
        let (width, height) = screenshot_size(display.width, display.height);
        Self {
            display,
            width,
            height,
        }
    }

    /// A screenshot pixel → the screen's coordinates (the pixel's centre).
    pub fn to_screen(&self, (x, y): (i32, i32)) -> Result<(i32, i32), String> {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return Err(format!(
                "({x}, {y}) is outside the screen: screenshots are {}×{} pixels, so x must be \
                 0–{} and y 0–{}.",
                self.width,
                self.height,
                self.width - 1,
                self.height - 1
            ));
        }
        let d = &self.display;
        let sx = (f64::from(x) + 0.5) * f64::from(d.width) / f64::from(self.width);
        let sy = (f64::from(y) + 0.5) * f64::from(d.height) / f64::from(self.height);
        let px = (sx.floor() as i64).clamp(0, i64::from(d.width) - 1);
        let py = (sy.floor() as i64).clamp(0, i64::from(d.height) - 1);
        Ok((d.x + px as i32, d.y + py as i32))
    }

    /// The screen's coordinates → a screenshot pixel; `None` when it's on another screen.
    pub fn to_model(&self, (x, y): (i32, i32)) -> Option<(i32, i32)> {
        let d = &self.display;
        if !d.contains(x, y) {
            return None;
        }
        let mx = f64::from(x - d.x) * f64::from(self.width) / f64::from(d.width);
        let my = f64::from(y - d.y) * f64::from(self.height) / f64::from(d.height);
        Some((
            (mx.floor() as i32).clamp(0, self.width as i32 - 1),
            (my.floor() as i32).clamp(0, self.height as i32 - 1),
        ))
    }

    /// A screenshot region → the screen's (x, y, width, height), relative to the screen.
    pub fn region_on_screen(&self, r: [i32; 4]) -> Result<(u32, u32, u32, u32), String> {
        let x1 = r[2].min(self.width as i32);
        let y1 = r[3].min(self.height as i32);
        if r[0] >= x1 || r[1] >= y1 {
            return Err(format!(
                "The region is outside the screenshot ({}×{}).",
                self.width, self.height
            ));
        }
        let d = &self.display;
        let fx = f64::from(d.width) / f64::from(self.width);
        let fy = f64::from(d.height) / f64::from(self.height);
        let sx = (f64::from(r[0]) * fx).floor() as u32;
        let sy = (f64::from(r[1]) * fy).floor() as u32;
        let ex = ((f64::from(x1) * fx).ceil() as u32).min(d.width);
        let ey = ((f64::from(y1) * fy).ceil() as u32).min(d.height);
        Ok((sx, sy, (ex - sx).max(1), (ey - sy).max(1)))
    }
}

// ---- Approvals and activity -----------------------------------------------------------------

/// What a computer approval card asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ComputerApprovalKind {
    /// Start controlling the computer in this chat.
    Session,
    /// Use this app.
    App,
}

/// The computer part of a [`crate::tools::ToolApprovalRequest`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ComputerApproval {
    pub kind: ComputerApprovalKind,
    /// The app, for an app card.
    pub app: Option<ComputerApp>,
    /// The screen models will see ("Dell U2720Q (2560×1440)").
    pub display: Option<String>,
    /// What the model wants to do ("Clicking at (512, 300)").
    pub action: String,
    /// The shortcut that stops it.
    pub panic_shortcut: String,
}

/// What a chat that controls the computer is doing (for the control bar).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ComputerActivityState {
    /// Acting (the bar shows the action).
    Working,
    /// Waiting for the user (an approval card); Oxide's window comes back.
    Waiting,
    /// Done: the bar goes when no chat is working.
    Ended,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ComputerActivity {
    pub conversation_id: String,
    pub state: ComputerActivityState,
    /// The current action ("Typing “hello”").
    pub action: Option<String>,
}

/// Settings → Computer use.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ComputerStatus {
    /// This build can control the computer.
    pub supported: bool,
    pub displays: Vec<ComputerDisplay>,
    /// Why screens couldn't be listed.
    pub error: Option<String>,
    /// Chats controlling the computer now.
    pub active: Vec<String>,
    pub panic_shortcut: String,
    /// The panic shortcut couldn't be registered (another app has it).
    pub panic_error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn display(w: u32, h: u32) -> ComputerDisplay {
        ComputerDisplay {
            id: "\\\\.\\DISPLAY1".into(),
            name: "Main".into(),
            x: 0,
            y: 0,
            width: w,
            height: h,
            scale_factor: 1.5,
            primary: true,
        }
    }

    #[test]
    fn screenshots_are_scaled_to_fit_and_never_enlarged() {
        assert_eq!(screenshot_size(2560, 1440), (1366, 768));
        assert_eq!(screenshot_size(1920, 1080), (1366, 768));
        assert_eq!(screenshot_size(3840, 2160), (1366, 768));
        assert_eq!(screenshot_size(1280, 800), (1280, 800));
        assert_eq!(screenshot_size(1024, 768), (1024, 768));
        let (w, h) = screenshot_size(1920, 1200);
        assert!(u64::from(w) * u64::from(h) <= MAX_SHOT_PIXELS, "{w}×{h}");
        assert!(w <= MAX_SHOT_WIDTH && h <= MAX_SHOT_HEIGHT);
        // Portrait.
        let (w, h) = screenshot_size(1440, 2560);
        assert!(h <= MAX_SHOT_HEIGHT && w < h, "{w}×{h}");
    }

    #[test]
    fn coordinates_map_to_the_screen_and_back() {
        let f = Frame::new(display(2560, 1440));
        assert_eq!((f.width, f.height), (1366, 768));
        assert_eq!(f.to_screen((0, 0)), Ok((0, 0)));
        assert_eq!(f.to_screen((1365, 767)), Ok((2559, 1439)));
        assert_eq!(f.to_screen((683, 384)), Ok((1280, 720)));
        assert!(f.to_screen((1366, 10)).is_err());
        assert!(f.to_screen((-1, 10)).is_err());
        // Round trip within one screenshot pixel.
        for p in [(10, 20), (683, 384), (1300, 700)] {
            let s = f.to_screen(p).expect("on screen");
            assert_eq!(f.to_model(s), Some(p));
        }
        // A second screen to the left of the main one.
        let mut left = display(1920, 1080);
        left.x = -1920;
        left.y = 100;
        let f = Frame::new(left);
        assert_eq!(f.to_screen((0, 0)), Ok((-1920, 100)));
        assert_eq!(f.to_model((-1, 100)), Some((1365, 0)));
        assert_eq!(f.to_model((0, 100)), None);
        // Zoom regions.
        let f = Frame::new(display(2560, 1440));
        assert_eq!(f.region_on_screen([0, 0, 683, 384]), Ok((0, 0, 1280, 720)));
        assert!(f.region_on_screen([2000, 0, 2100, 10]).is_err());
    }

    #[test]
    fn displays_are_picked_from_settings_else_the_main_one() {
        let mut a = display(1920, 1080);
        a.id = "A".into();
        a.primary = false;
        let mut b = display(2560, 1440);
        b.id = "B".into();
        let all = vec![a, b];
        assert_eq!(pick_display(&all, None).map(|d| d.id.as_str()), Some("B"));
        assert_eq!(
            pick_display(&all, Some("A")).map(|d| d.id.as_str()),
            Some("A")
        );
        assert_eq!(
            pick_display(&all, Some("gone")).map(|d| d.id.as_str()),
            Some("B")
        );
        assert_eq!(pick_display(&[], None), None);
    }

    #[test]
    fn keys_are_read_the_way_models_write_them() {
        use KeyName::*;
        assert_eq!(parse_combo("ctrl+s"), Ok(vec![Control, Char('s')]));
        assert_eq!(parse_combo("ctrl+S"), Ok(vec![Control, Char('s')]));
        assert_eq!(parse_combo("A"), Ok(vec![Char('A')]));
        assert_eq!(parse_combo("Return"), Ok(vec![Return]));
        assert_eq!(parse_combo("super"), Ok(vec![Meta]));
        assert_eq!(parse_combo("cmd+shift+4"), Ok(vec![Meta, Shift, Char('4')]));
        assert_eq!(parse_combo("Page_Down"), Ok(vec![PageDown]));
        assert_eq!(parse_combo("F5"), Ok(vec![F(5)]));
        assert_eq!(parse_combo("alt+F4"), Ok(vec![Alt, F(4)]));
        assert_eq!(parse_combo("ctrl++"), Ok(vec![Control, Char('+')]));
        assert_eq!(parse_combo("+"), Ok(vec![Char('+')]));
        assert_eq!(parse_combo("ctrl+plus"), Ok(vec![Control, Char('+')]));
        assert_eq!(parse_combo("KP_Enter"), Ok(vec![Return]));
        assert!(parse_combo("hyper").is_err());
        assert!(parse_combo("F25").is_err());
        assert!(parse_combo("").is_err());
        assert_eq!(
            parse_keys("ctrl+a Delete"),
            Ok(vec![vec![Control, Char('a')], vec![Delete]])
        );
        assert_eq!(parse_keys(" "), Ok(vec![vec![Space]]));
        assert_eq!(
            parse_modifiers(Some("ctrl+shift")),
            Ok(vec![Control, Shift])
        );
        assert_eq!(parse_modifiers(None), Ok(vec![]));
        assert!(parse_modifiers(Some("a")).is_err());
    }

    #[test]
    fn actions_are_read_and_checked() {
        let a = parse_action(&json!({"action": "left_click", "coordinate": [512.4, 300]}));
        assert_eq!(
            a,
            Ok(ComputerAction::Click {
                button: MouseButton::Left,
                count: 1,
                at: Some((512, 300)),
                modifiers: vec![]
            })
        );
        let a = parse_action(&json!({"action": "double_click", "text": "shift"}))
            .expect("double click");
        assert_eq!(a.name(), "double_click");
        assert_eq!(a.label(), "Double-clicking");
        let a = parse_action(
            &json!({"action": "left_click_drag", "start_coordinate": [1, 2], "coordinate": [3, 4]}),
        )
        .expect("drag");
        assert_eq!(a.name(), "left_click_drag");
        assert!(parse_action(&json!({"action": "left_click_drag", "coordinate": [3, 4]})).is_err());
        let a = parse_action(&json!({"action": "scroll", "scroll_direction": "up", "scroll_amount": 5, "coordinate": {"x": 1, "y": 2}}))
            .expect("scroll");
        assert_eq!(
            a,
            ComputerAction::Scroll {
                direction: ScrollDirection::Up,
                amount: 5,
                at: Some((1, 2)),
                modifiers: vec![]
            }
        );
        let a =
            parse_action(&json!({"action": "key", "text": "ctrl+s", "repeat": 2})).expect("key");
        assert_eq!(a.label(), "Pressing ctrl+s ×2");
        assert!(parse_action(&json!({"action": "key", "text": "ctrl+s", "repeat": 101})).is_err());
        let a = parse_action(&json!({"action": "type", "text": "Hello\nworld"})).expect("type");
        assert_eq!(a.label(), "Typing “Hello world”");
        assert!(parse_action(&json!({"action": "type", "text": ""})).is_err());
        assert_eq!(
            parse_action(&json!({"action": "wait", "duration": 1.5})),
            Ok(ComputerAction::Wait { duration_ms: 1500 })
        );
        assert_eq!(
            parse_action(&json!({"action": "wait"})),
            Ok(ComputerAction::Wait { duration_ms: 1000 })
        );
        assert!(parse_action(&json!({"action": "wait", "duration": 301})).is_err());
        assert!(parse_action(&json!({"action": "hold_key", "text": "shift"})).is_err());
        assert_eq!(
            parse_action(&json!({"action": "zoom", "region": [0, 0, 100, 50]})),
            Ok(ComputerAction::Zoom {
                region: [0, 0, 100, 50]
            })
        );
        assert!(parse_action(&json!({"action": "zoom", "region": [10, 0, 5, 50]})).is_err());
        assert!(parse_action(&json!({"action": "left_click", "coordinate": [-1, 2]})).is_err());
        assert!(parse_action(&json!({"action": "fly"})).is_err());
        assert!(parse_action(&json!({})).is_err());
        // Every action the toolset names is read.
        for name in COMPUTER_ACTIONS {
            let input = json!({
                "action": name, "coordinate": [1, 1], "start_coordinate": [0, 0],
                "text": if name == "type" { "x" } else { "shift" }, "duration": 1,
                "region": [0, 0, 2, 2]
            });
            let a = parse_action(&input).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(a.name(), name);
        }
        assert_eq!(
            computer_schema()["properties"]["action"]["enum"]
                .as_array()
                .map(Vec::len),
            Some(17)
        );
    }

    #[test]
    fn looking_never_needs_an_app() {
        assert!(ComputerAction::Screenshot.only_looks());
        assert!(ComputerAction::Wait { duration_ms: 1 }.only_looks());
        assert!(!ComputerAction::Type { text: "x".into() }.only_looks());
        assert!(ComputerAction::Type { text: "x".into() }.shows_result());
        assert!(!ComputerAction::Screenshot.shows_result());
    }

    #[test]
    fn apps_are_named_grouped_and_blocked() {
        let id = |exe: &str, name: &str| AppIdentity {
            exe: exe.into(),
            name: name.into(),
            title: String::new(),
            pid: 7,
        };
        assert_eq!(
            app_of(&id("C:\\Windows\\System32\\notepad.exe", "Notepad")),
            ComputerApp {
                id: "notepad.exe".into(),
                name: "Notepad".into()
            }
        );
        assert_eq!(app_of(&id("Code.exe", "")).name, "code");
        assert_eq!(
            app_of(&id("explorer.exe", "Windows Explorer")).id,
            SHELL_APP_ID
        );
        assert_eq!(app_of(&id("SearchHost.exe", "")).id, SHELL_APP_ID);
        let mut uwp = id("ApplicationFrameHost.exe", "Application Frame Host");
        uwp.title = "Calculator".into();
        assert_eq!(app_of(&uwp).name, "Calculator");
        assert!(blocked_app(&id("notepad.exe", "Notepad"), 1).is_none());
        assert!(
            blocked_app(&id("notepad.exe", "Notepad"), 7).is_some(),
            "own pid"
        );
        assert!(blocked_app(&id("C:\\x\\KeePassXC.exe", "KeePassXC"), 1).is_some());
        assert!(blocked_app(&id("CredentialUIBroker.exe", ""), 1).is_some());
    }

    #[test]
    fn password_and_payment_fields_are_never_typed_into() {
        let f = |name: &str, id: &str| FieldInfo {
            name: name.into(),
            automation_id: id.into(),
            ..FieldInfo::default()
        };
        assert!(
            sensitive_field(&FieldInfo {
                is_password: true,
                ..FieldInfo::default()
            })
            .is_some()
        );
        assert!(sensitive_field(&f("Password", "")).is_some());
        assert!(sensitive_field(&f("Adgangskode", "")).is_some());
        assert!(sensitive_field(&f("", "cc-number")).is_some());
        assert!(sensitive_field(&f("Card number", "")).is_some());
        assert!(sensitive_field(&f("CVV", "")).is_some());
        assert!(sensitive_field(&f("Enter PIN", "")).is_some());
        assert!(sensitive_field(&f("IBAN", "")).is_some());
        assert!(sensitive_field(&f("Text editor", "RichEditD2DPT")).is_none());
        assert!(
            sensitive_field(&f("Spinner", "")).is_none(),
            "pin only as a word"
        );
        assert!(sensitive_field(&f("Search", "searchbox")).is_none());
    }

    #[test]
    fn card_and_bank_numbers_are_never_typed() {
        assert!(sensitive_text("4111 1111 1111 1111").is_some());
        assert!(sensitive_text("my card is 4111-1111-1111-1111 thanks").is_some());
        assert!(sensitive_text("5555555555554444").is_some());
        assert!(sensitive_text("DK50 0040 0440 1162 43").is_some());
        assert!(sensitive_text("GB82WEST12345698765432").is_some());
        assert!(sensitive_text("Hello world").is_none());
        assert!(
            sensitive_text("Order 1234567890123 shipped").is_none(),
            "fails Luhn"
        );
        assert!(sensitive_text("Call 12 34 56 78").is_none());
        assert!(
            sensitive_text("DK51 0040 0440 1162 43").is_none(),
            "bad check"
        );
    }

    #[test]
    fn settings_are_tidied() {
        let s = ComputerSettings {
            display: Some("  ".into()),
            always_allowed_apps: vec![
                ComputerApp {
                    id: " Notepad.EXE ".into(),
                    name: " Notepad ".into(),
                },
                ComputerApp {
                    id: "notepad.exe".into(),
                    name: "again".into(),
                },
                ComputerApp {
                    id: "".into(),
                    name: "x".into(),
                },
            ],
            ..ComputerSettings::default()
        }
        .normalized();
        assert_eq!(s.display, None);
        assert_eq!(
            s.always_allowed_apps,
            vec![ComputerApp {
                id: "notepad.exe".into(),
                name: "Notepad".into()
            }]
        );
        assert!(s.always_allows(&ComputerApp {
            id: "notepad.exe".into(),
            name: String::new()
        }));
        assert!(!ComputerSettings::default().enabled);
    }
}
