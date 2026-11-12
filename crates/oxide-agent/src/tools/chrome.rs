//! The Chrome extension's tools (Phase 14), as models get them: `chrome__tabs_list`,
//! `chrome__navigate`, `chrome__read_page`, `chrome__click`, … They run in the user's own
//! Chrome through `oxide-bridge`; the descriptions here match `extension/src/background.ts`.

use oxide_core::chrome::{
    CHROME_CLICK, CHROME_FIND, CHROME_FORM_INPUT, CHROME_GET_CONSOLE, CHROME_NAVIGATE,
    CHROME_PRESS_KEY, CHROME_READ_PAGE, CHROME_SCREENSHOT, CHROME_SCROLL, CHROME_SERVER,
    CHROME_TABS_CLOSE, CHROME_TABS_CREATE, CHROME_TABS_LIST, CHROME_TABS_SELECT, CHROME_TOOLS,
    CHROME_TYPE, chrome_tool_reads_only,
};
use serde_json::{Value, json};

use super::{ToolEntry, ToolKind, api_name};

/// `read_page`'s own default length (`READ_DEFAULT` in the extension).
pub const READ_DEFAULT_CHARS: usize = 20_000;

fn describe(tool: &str) -> (&'static str, &'static str, Value) {
    let empty = json!({ "type": "object", "properties": {} });
    match tool {
        CHROME_TABS_LIST => (
            "List tabs",
            "List the tabs open in the user's Chrome: id, title and address, and which one this \
             chat works in.",
            empty,
        ),
        CHROME_TABS_CREATE => (
            "Open a tab",
            "Open a new tab in the user's Chrome (optionally at an address) and work in it from \
             now on. Returns the page like chrome__navigate.",
            json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "Address to open, or search words. Leave out for an empty tab." }
                }
            }),
        ),
        CHROME_TABS_SELECT => (
            "Use a tab",
            "Work in one of the user's existing tabs (by its id from chrome__tabs_list), e.g. a \
             page they have open and are signed in to.",
            json!({
                "type": "object",
                "properties": { "tab_id": { "type": "integer", "description": "The tab's id." } },
                "required": ["tab_id"]
            }),
        ),
        CHROME_TABS_CLOSE => (
            "Close a tab",
            "Close a tab Oxide opened for this chat (default: the one it works in now). The \
             user's own tabs can't be closed.",
            json!({
                "type": "object",
                "properties": { "tab_id": { "type": "integer", "description": "The tab's id." } }
            }),
        ),
        CHROME_NAVIGATE => (
            "Open a page in Chrome",
            "Open an address in this chat's tab in the user's Chrome (a new tab when there is \
             none) and return the page's text with links, buttons and fields numbered for \
             chrome__click and chrome__type. Search words instead of an address search Google. \
             \"back\", \"forward\" and \"reload\" move in the tab's history.",
            json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "The address, search words, or back / forward / reload." }
                },
                "required": ["url"]
            }),
        ),
        CHROME_READ_PAGE => (
            "Read the page",
            "Read this chat's tab as text: headings, text, and numbered links, buttons and fields \
             ([3] button \"Search\"). Numbers change when the page changes, so read again after \
             acting. Long pages are cut; pass start to read on, or filter \"interactive\" for the \
             numbered elements only.",
            json!({
                "type": "object",
                "properties": {
                    "start": { "type": "integer", "minimum": 0, "description": "Character to start at." },
                    "max_chars": { "type": "integer", "minimum": 1000, "maximum": 100000,
                                   "description": "Most characters to return (default 20000)." },
                    "filter": { "type": "string", "enum": ["all", "interactive"],
                                "description": "interactive: only the numbered elements." }
                }
            }),
        ),
        CHROME_FIND => (
            "Find on the page",
            "Find links, buttons and fields on the page matching words (\"search box\", \"add to \
             cart\"), numbered for chrome__click and chrome__type, plus lines of text that \
             mention them.",
            json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "What to look for." },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 50 }
                },
                "required": ["query"]
            }),
        ),
        CHROME_CLICK => (
            "Click",
            "Click an element by its number (ref), by its visible text, or at x/y pixels of the \
             last chrome__screenshot. Returns the page after the click (links that open a new \
             tab are followed).",
            json!({
                "type": "object",
                "properties": {
                    "ref": { "type": "integer", "description": "The element's number from read_page or find." },
                    "text": { "type": "string", "description": "Visible text of the element, when there is no number." },
                    "x": { "type": "number", "description": "Pixels from the left of the screenshot." },
                    "y": { "type": "number", "description": "Pixels from the top of the screenshot." }
                }
            }),
        ),
        CHROME_TYPE => (
            "Type",
            "Type into a text field (by number, or by its label or placeholder). Replaces what is \
             there unless clear is false; submit presses Enter afterwards and returns the page. \
             Password and payment-card fields are refused: ask the user to fill those in.",
            json!({
                "type": "object",
                "properties": {
                    "text": { "type": "string", "description": "What to type." },
                    "ref": { "type": "integer", "description": "The field's number." },
                    "field": { "type": "string", "description": "The field's label or placeholder, when there is no number." },
                    "submit": { "type": "boolean", "description": "Press Enter afterwards." },
                    "clear": { "type": "boolean", "description": "Replace the field's text (default) instead of adding to it." }
                },
                "required": ["text"]
            }),
        ),
        CHROME_PRESS_KEY => (
            "Press a key",
            "Press a key or combination on the focused element or a numbered one: Enter, Escape, \
             Tab, ArrowDown, PageDown, ctrl+a, …",
            json!({
                "type": "object",
                "properties": {
                    "key": { "type": "string", "description": "The key, e.g. Enter or ctrl+a." },
                    "ref": { "type": "integer", "description": "The element to press it on." }
                },
                "required": ["key"]
            }),
        ),
        CHROME_SCROLL => (
            "Scroll",
            "Scroll the page (up, down, left, right, top, bottom; amount in screens) or a \
             numbered element into view.",
            json!({
                "type": "object",
                "properties": {
                    "direction": { "type": "string", "enum": ["up", "down", "left", "right", "top", "bottom"] },
                    "amount": { "type": "number", "description": "Screens to scroll (default 1)." },
                    "ref": { "type": "integer", "description": "Scroll this element into view instead." }
                }
            }),
        ),
        CHROME_FORM_INPUT => (
            "Set a form field",
            "Set a numbered form field: a select's option (by its text or value), a checkbox or \
             radio button (true or false), or any other field's value.",
            json!({
                "type": "object",
                "properties": {
                    "ref": { "type": "integer", "description": "The field's number." },
                    "value": { "description": "The option, true/false, or the value." }
                },
                "required": ["ref", "value"]
            }),
        ),
        CHROME_SCREENSHOT => (
            "Screenshot",
            "Take a screenshot of the visible part of this chat's tab.",
            empty,
        ),
        CHROME_GET_CONSOLE => (
            "Console messages",
            "Read the page's console messages and uncaught errors (recorded while Oxide works on \
             the site, from each page's start). pattern filters them (a regular expression).",
            json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string" },
                    "only_errors": { "type": "boolean" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 500 },
                    "clear": { "type": "boolean", "description": "Forget the messages after reading them." }
                }
            }),
        ),
        _ => ("Chrome", "A Chrome tool.", empty),
    }
}

/// One Chrome tool as models get it.
pub fn chrome_entry(tool: &str) -> ToolEntry {
    let (title, description, schema) = describe(tool);
    ToolEntry {
        api_name: api_name(CHROME_SERVER, tool),
        kind: ToolKind::Chrome {
            tool: tool.to_owned(),
        },
        title: Some(title.into()),
        description: format!("[chrome] {description}"),
        schema,
        read_only: chrome_tool_reads_only(tool),
        destructive: tool == CHROME_TABS_CLOSE,
    }
}

/// All the extension's tools, in the order models get them.
pub fn chrome_entries() -> Vec<ToolEntry> {
    CHROME_TOOLS.iter().map(|t| chrome_entry(t)).collect()
}

/// A short label for the usage log: the address, what was clicked or typed.
pub fn chrome_label(tool: &str, input: &Value) -> Option<String> {
    let s = |k: &str| {
        input
            .get(k)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(|v| v.chars().take(200).collect::<String>())
    };
    let r = |k: &str| {
        input
            .get(k)
            .and_then(Value::as_u64)
            .map(|r| format!("[{r}]"))
    };
    match tool {
        CHROME_NAVIGATE | CHROME_TABS_CREATE => s("url"),
        CHROME_FIND => s("query"),
        CHROME_CLICK => r("ref").or_else(|| s("text")),
        CHROME_TYPE => s("text").map(|t| format!("“{t}”")),
        CHROME_PRESS_KEY => s("key"),
        CHROME_FORM_INPUT => r("ref"),
        CHROME_TABS_SELECT | CHROME_TABS_CLOSE => r("tab_id"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_are_namespaced_and_only_looking_is_read_only() {
        let e = chrome_entries();
        assert_eq!(e.len(), 14);
        assert!(e.iter().all(|x| x.api_name.starts_with("chrome__")));
        let ro: Vec<&str> = e.iter().filter(|x| x.read_only).map(|x| x.tool()).collect();
        assert_eq!(
            ro,
            vec![
                "tabs_list",
                "tabs_select",
                "read_page",
                "find",
                "scroll",
                "screenshot",
                "get_console"
            ]
        );
        let nav = e.iter().find(|x| x.tool() == "navigate").expect("navigate");
        assert_eq!(nav.server(), Some("chrome"));
        assert_eq!(nav.grant_key(), "builtin/chrome__navigate");
        assert_eq!(nav.schema["required"], json!(["url"]));
        assert!(e.iter().any(|x| x.destructive && x.tool() == "tabs_close"));
    }

    #[test]
    fn labels_say_what_the_call_was_about() {
        assert_eq!(
            chrome_label(CHROME_NAVIGATE, &json!({"url": "https://a.b"})).as_deref(),
            Some("https://a.b")
        );
        assert_eq!(
            chrome_label(CHROME_CLICK, &json!({"ref": 3})).as_deref(),
            Some("[3]")
        );
        assert_eq!(
            chrome_label(CHROME_TYPE, &json!({"text": "rye"})).as_deref(),
            Some("“rye”")
        );
        assert_eq!(chrome_label(CHROME_SCREENSHOT, &json!({})), None);
    }
}
