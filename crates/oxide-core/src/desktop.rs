//! Desktop integration: what the tray, the quick entry window, deep links and the
//! operating system ask the main window to do, and the pure parsing behind them.
//!
//! Deep links use the `oxide://` scheme:
//! - `oxide://chat/<id>` opens a chat.
//! - `oxide://new` opens a new chat; `oxide://new?q=<text>` fills in its composer.
//! - `oxide://settings` (or `oxide://settings/<section>`) opens Settings.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::message::ContentBlock;

/// The URL scheme Oxide registers with the operating system.
pub const DEEP_LINK_SCHEME: &str = "oxide";

/// Default global shortcut for quick entry (Alt+Space; Option+Space on macOS).
pub const DEFAULT_QUICK_ENTRY_SHORTCUT: &str = "Alt+Space";

/// Longest text a deep link may put into the composer.
pub const MAX_DEEP_LINK_TEXT: usize = 10_000;

/// Something the main window should do, sent by the tray, a menu, quick entry or a deep link
/// (`events.desktopRequest`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum DesktopAction {
    /// Open this chat.
    #[serde(rename_all = "camelCase")]
    OpenChat { conversation_id: String },
    /// Open a new chat, with `text` in the composer when given.
    NewChat { text: Option<String> },
    /// Open Settings, at `section` when given.
    OpenSettings { section: Option<String> },
    /// Open Scheduled tasks, at this task when given.
    #[serde(rename_all = "camelCase")]
    OpenScheduled { task_id: Option<String> },
}

/// Parses an `oxide://` link. Anything else (another scheme, an unknown path, a malformed
/// chat id) gives `None`.
pub fn parse_deep_link(url: &str) -> Option<DesktopAction> {
    let url = url.trim();
    let prefix = format!("{DEEP_LINK_SCHEME}:");
    if url.len() < prefix.len() || !url[..prefix.len()].eq_ignore_ascii_case(&prefix) {
        return None;
    }
    // `oxide://x`, and the `oxide:x` form some launchers pass on.
    let rest = url[prefix.len()..].trim_start_matches('/');
    let (path, query) = match rest.split_once('?') {
        Some((p, q)) => (p, q),
        None => (rest, ""),
    };
    let query = query.split('#').next().unwrap_or("");
    let path = path.split('#').next().unwrap_or("");
    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    match parts.as_slice() {
        [first, id] if first.eq_ignore_ascii_case("chat") => {
            let id = percent_decode(id)?;
            valid_id(&id).then_some(DesktopAction::OpenChat {
                conversation_id: id,
            })
        }
        [] => Some(DesktopAction::NewChat { text: None }),
        [first] if first.eq_ignore_ascii_case("new") => {
            let text = query_value(query, "q")
                .map(|t| t.chars().take(MAX_DEEP_LINK_TEXT).collect::<String>())
                .filter(|t| !t.trim().is_empty());
            Some(DesktopAction::NewChat { text })
        }
        [first] if first.eq_ignore_ascii_case("scheduled") => {
            Some(DesktopAction::OpenScheduled { task_id: None })
        }
        [first, id] if first.eq_ignore_ascii_case("scheduled") => {
            let id = percent_decode(id)?;
            valid_id(&id).then_some(DesktopAction::OpenScheduled { task_id: Some(id) })
        }
        [first] if first.eq_ignore_ascii_case("settings") => {
            Some(DesktopAction::OpenSettings { section: None })
        }
        [first, section] if first.eq_ignore_ascii_case("settings") => {
            let section = percent_decode(section)?.to_ascii_lowercase();
            Some(DesktopAction::OpenSettings {
                section: valid_id(&section).then_some(section),
            })
        }
        _ => None,
    }
}

/// The first `oxide://` link among command-line arguments (how Windows and Linux hand a
/// link to the app). The first argument, the program itself, is skipped.
pub fn deep_link_in_args<S: AsRef<str>>(args: &[S]) -> Option<DesktopAction> {
    args.iter()
        .skip(1)
        .find_map(|a| parse_deep_link(a.as_ref()))
}

/// Ids are UUIDs in practice; allow letters, digits, `-` and `_`, up to 64 characters.
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// The decoded value of `key` in a query string (`a=1&q=hello+world`).
fn query_value(query: &str, key: &str) -> Option<String> {
    query.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        (k == key)
            .then(|| percent_decode(&v.replace('+', " ")))
            .flatten()
    })
}

/// Decodes `%XX` escapes (UTF-8). Invalid escapes or bytes give `None`.
fn percent_decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                let hex = s.get(i + 1..i + 3)?;
                out.push(u8::from_str_radix(hex, 16).ok()?);
                i += 3;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

/// Normalizes a global shortcut such as `ctrl+shift+k` to `Ctrl+Shift+K`: known modifier
/// names in a fixed order, then one key. `None` when it has no modifier, no key, more than
/// one key, or an empty part. The operating system can still refuse a valid one (taken).
pub fn clean_shortcut(input: &str) -> Option<String> {
    const ORDER: [&str; 5] = ["CmdOrCtrl", "Ctrl", "Alt", "Shift", "Super"];
    let mut mods: Vec<&str> = Vec::new();
    let mut key: Option<String> = None;
    for raw in input.split('+') {
        let part = raw.trim();
        if part.is_empty() {
            return None;
        }
        let m = match part.to_ascii_lowercase().as_str() {
            "cmdorctrl" | "cmdorcontrol" | "commandorctrl" | "commandorcontrol" => {
                Some("CmdOrCtrl")
            }
            "ctrl" | "control" => Some("Ctrl"),
            "alt" | "option" => Some("Alt"),
            "shift" => Some("Shift"),
            "super" | "cmd" | "command" | "meta" | "win" => Some("Super"),
            _ => None,
        };
        match m {
            Some(m) => {
                if !mods.contains(&m) {
                    mods.push(m);
                }
            }
            None => {
                if key.is_some() || !part.chars().all(|c| c.is_ascii_graphic()) {
                    return None;
                }
                key = Some(clean_key(part));
            }
        }
    }
    let key = key?;
    if mods.is_empty() {
        return None;
    }
    let mut out: Vec<String> = ORDER
        .iter()
        .filter(|m| mods.contains(m))
        .map(|m| (*m).to_owned())
        .collect();
    out.push(key);
    Some(out.join("+"))
}

/// `a` → `A`, `space` → `Space`, `f5` → `F5`: the first letter upper-cased, the rest kept.
fn clean_key(key: &str) -> String {
    let mut chars = key.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
        None => String::new(),
    }
}

/// Longest reply preview in a notification, in characters.
pub const NOTIFICATION_PREVIEW_CHARS: usize = 140;

/// The body of a "reply finished" notification: the start of the reply's text on one line,
/// or a fallback when it has no text (only tool calls, say).
pub fn reply_preview(content: &[ContentBlock]) -> String {
    let text = content
        .iter()
        .filter_map(|b| match b {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ");
    let one_line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.is_empty() {
        return "The reply is ready.".into();
    }
    let mut preview: String = one_line.chars().take(NOTIFICATION_PREVIEW_CHARS).collect();
    if one_line.chars().count() > NOTIFICATION_PREVIEW_CHARS {
        preview = preview.trim_end().to_owned();
        preview.push('…');
    }
    preview
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chat(id: &str) -> Option<DesktopAction> {
        Some(DesktopAction::OpenChat {
            conversation_id: id.into(),
        })
    }

    #[test]
    fn chat_links() {
        let id = "0193f0a2-7c1e-7b3a-9f00-1234567890ab";
        assert_eq!(parse_deep_link(&format!("oxide://chat/{id}")), chat(id));
        assert_eq!(parse_deep_link(&format!("OXIDE://Chat/{id}/")), chat(id));
        assert_eq!(parse_deep_link(&format!("oxide:chat/{id}")), chat(id));
        assert_eq!(parse_deep_link(&format!(" oxide://chat/{id}#x ")), chat(id));
        assert_eq!(parse_deep_link("oxide://chat/"), None);
        assert_eq!(parse_deep_link("oxide://chat/../../etc"), None);
        assert_eq!(parse_deep_link("oxide://chat/a%20b"), None);
        assert_eq!(
            parse_deep_link(&format!("oxide://chat/{}", "a".repeat(65))),
            None
        );
    }

    #[test]
    fn new_chat_links() {
        let new = |t: Option<&str>| {
            Some(DesktopAction::NewChat {
                text: t.map(Into::into),
            })
        };
        assert_eq!(parse_deep_link("oxide://new"), new(None));
        assert_eq!(parse_deep_link("oxide://"), new(None));
        assert_eq!(parse_deep_link("oxide://new/?q="), new(None));
        assert_eq!(
            parse_deep_link("oxide://new?q=Hello%2C+world%21"),
            new(Some("Hello, world!"))
        );
        assert_eq!(
            parse_deep_link("oxide://new?x=1&q=%C3%A6%C3%B8%C3%A5"),
            new(Some("æøå"))
        );
        // A broken escape drops the text, not the link.
        assert_eq!(parse_deep_link("oxide://new?q=%zz"), new(None));
        let long = format!("oxide://new?q={}", "a".repeat(MAX_DEEP_LINK_TEXT + 5));
        let Some(DesktopAction::NewChat { text: Some(t) }) = parse_deep_link(&long) else {
            panic!("expected a new chat");
        };
        assert_eq!(t.len(), MAX_DEEP_LINK_TEXT);
    }

    #[test]
    fn settings_and_other_links() {
        assert_eq!(
            parse_deep_link("oxide://settings"),
            Some(DesktopAction::OpenSettings { section: None })
        );
        assert_eq!(
            parse_deep_link("oxide://settings/Desktop"),
            Some(DesktopAction::OpenSettings {
                section: Some("desktop".into())
            })
        );
        assert_eq!(parse_deep_link("https://example.com/chat/1"), None);
        assert_eq!(parse_deep_link("oxid://chat/1"), None);
        assert_eq!(parse_deep_link("oxide://unknown/thing"), None);
        assert_eq!(parse_deep_link(""), None);
    }

    #[test]
    fn scheduled_links() {
        assert_eq!(
            parse_deep_link("oxide://scheduled"),
            Some(DesktopAction::OpenScheduled { task_id: None })
        );
        assert_eq!(
            parse_deep_link("oxide://Scheduled/abc-1"),
            Some(DesktopAction::OpenScheduled {
                task_id: Some("abc-1".into())
            })
        );
        assert_eq!(parse_deep_link("oxide://scheduled/a%20b"), None);
    }

    #[test]
    fn links_in_arguments() {
        let args = ["C:\\Oxide\\oxide.exe", "--hidden", "oxide://chat/abc"];
        assert_eq!(deep_link_in_args(&args), chat("abc"));
        // The program path is never read as a link.
        assert_eq!(deep_link_in_args(&["oxide://chat/abc"]), None);
        assert_eq!(deep_link_in_args(&["oxide.exe", "--hidden"]), None);
    }

    #[test]
    fn shortcuts_are_cleaned() {
        assert_eq!(clean_shortcut("Alt+Space").as_deref(), Some("Alt+Space"));
        assert_eq!(
            clean_shortcut("shift + ctrl + k").as_deref(),
            Some("Ctrl+Shift+K")
        );
        assert_eq!(clean_shortcut("option+space").as_deref(), Some("Alt+Space"));
        assert_eq!(
            clean_shortcut("Cmd+Shift+f5").as_deref(),
            Some("Shift+Super+F5")
        );
        assert_eq!(
            clean_shortcut("CommandOrControl+O").as_deref(),
            Some("CmdOrCtrl+O")
        );
        assert_eq!(clean_shortcut("Ctrl+Ctrl+1").as_deref(), Some("Ctrl+1"));
        assert_eq!(
            clean_shortcut("ctrl+ArrowUp").as_deref(),
            Some("Ctrl+ArrowUp")
        );
        assert_eq!(clean_shortcut("K"), None, "needs a modifier");
        assert_eq!(clean_shortcut("Ctrl+Alt"), None, "needs a key");
        assert_eq!(clean_shortcut("Ctrl+A+B"), None, "one key only");
        assert_eq!(clean_shortcut("Ctrl++"), None);
        assert_eq!(clean_shortcut(""), None);
        assert_eq!(clean_shortcut("Ctrl+æ"), None);
    }

    #[test]
    fn reply_previews() {
        assert_eq!(
            reply_preview(&[ContentBlock::text("  Hello\n\nthere  ")]),
            "Hello there"
        );
        assert_eq!(reply_preview(&[]), "The reply is ready.");
        let long = reply_preview(&[ContentBlock::text("word ".repeat(100))]);
        assert!(long.ends_with('…'));
        assert!(long.chars().count() <= NOTIFICATION_PREVIEW_CHARS + 1);
    }

    #[test]
    fn actions_serialize_for_the_frontend() {
        let json = serde_json::to_value(DesktopAction::OpenChat {
            conversation_id: "c1".into(),
        })
        .expect("serializes");
        assert_eq!(
            json,
            serde_json::json!({"type": "openChat", "conversationId": "c1"})
        );
    }
}
