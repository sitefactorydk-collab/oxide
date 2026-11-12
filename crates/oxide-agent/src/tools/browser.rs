//! The browser agent's tools (Phase 13), as models get them: `browser__browser_task`,
//! `browser__navigate`, `browser__read_page`, `browser__click`, `browser__type`,
//! `browser__screenshot` and `browser__close`.
//!
//! The work happens in `oxide-pyenv` (a per-chat MCP server in the managed Python
//! environment). The descriptions here match `python/oxide_browser_agent/server.py`; the
//! real-browser test checks that the server offers exactly these tools.

use oxide_core::browser::{
    BROWSER_CLICK, BROWSER_CLOSE, BROWSER_NAVIGATE, BROWSER_READ_PAGE, BROWSER_SCREENSHOT,
    BROWSER_SEARCH, BROWSER_SERVER, BROWSER_TASK, BROWSER_TOOLS, BROWSER_TYPE,
    browser_tool_reads_only,
};
use serde_json::{Value, json};

use super::{ToolEntry, ToolKind, api_name};

/// `read_page`'s own default length (`READ_DEFAULT_CHARS` in the Python server).
pub const READ_DEFAULT_CHARS: usize = 20_000;

fn describe(tool: &str) -> (&'static str, &'static str, Value) {
    match tool {
        BROWSER_TASK => (
            "Browse autonomously",
            "Hand a browsing goal to an autonomous agent (Browser-Use) that opens pages, \
             clicks, types and reads until it is done, then reports what it found with \
             screenshots. Good for multi-step tasks on websites. For simple look-ups, \
             browser__navigate and browser__read_page are faster. Say exactly what to find or do \
             and what to report back.",
            json!({
                "type": "object",
                "properties": {
                    "goal": { "type": "string", "description": "What to do and what to report, in plain words." },
                    "max_steps": { "type": "integer", "minimum": 1, "maximum": 200,
                                   "description": "Most browser steps (default from Oxide's settings)." }
                },
                "required": ["goal"]
            }),
        ),
        BROWSER_NAVIGATE => (
            "Open a web page",
            "Open a URL in the browser and return the page's title, address and the start of \
             its text, with links, buttons and fields numbered for browser__click and \
             browser__type. Search words instead of a URL are searched on Google. An address \
             that doesn't exist (unknown site or 404) is searched for automatically, so open a \
             real result rather than guessing another address.",
            json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "The address, e.g. https://example.com, or search words." }
                },
                "required": ["url"]
            }),
        ),
        BROWSER_SEARCH => (
            "Search Google",
            "Search the web on Google in the browser and return the results page, with each \
             result's link numbered for browser__click (and Google's AI Overview first, when \
             there is one). Use it to find the right page whenever \
             you don't know its exact address, instead of guessing URLs.",
            json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "What to search for, in plain words." }
                },
                "required": ["query"]
            }),
        ),
        BROWSER_READ_PAGE => (
            "Read the page",
            "Read the current page as text: headings, text, and numbered links, buttons and \
             fields ([3] button \"Search\"). Numbers change when the page changes, so read again \
             after clicking. Long pages are cut; pass start to read on.",
            json!({
                "type": "object",
                "properties": {
                    "start": { "type": "integer", "minimum": 0, "description": "Character to start at." },
                    "max_chars": { "type": "integer", "minimum": 1000, "maximum": 100000,
                                   "description": "Most characters to return (default 20000)." }
                }
            }),
        ),
        BROWSER_CLICK => (
            "Click",
            "Click a link, button or other element: by its number from browser__read_page \
             (ref), or by its visible text. Returns the page after the click.",
            json!({
                "type": "object",
                "properties": {
                    "ref": { "type": "integer", "description": "The element's number from read_page." },
                    "text": { "type": "string", "description": "Visible text of the element, when there is no number." }
                }
            }),
        ),
        BROWSER_TYPE => (
            "Type",
            "Type into a text field (by its number from browser__read_page). Replaces what is \
             there unless clear is false; submit presses Enter afterwards. Returns the page.",
            json!({
                "type": "object",
                "properties": {
                    "text": { "type": "string", "description": "What to type." },
                    "ref": { "type": "integer", "description": "The field's number from read_page." },
                    "field": { "type": "string", "description": "Visible text or placeholder of the field, when there is no number." },
                    "submit": { "type": "boolean", "description": "Press Enter afterwards." },
                    "clear": { "type": "boolean", "description": "Replace the field's text (default) instead of adding to it." }
                },
                "required": ["text"]
            }),
        ),
        BROWSER_SCREENSHOT => (
            "Screenshot",
            "Take a screenshot of the current page (the visible part, or the whole page).",
            json!({
                "type": "object",
                "properties": {
                    "full_page": { "type": "boolean", "description": "The whole page instead of the visible part." }
                }
            }),
        ),
        _ => (
            "Close the browser",
            "Close the browser when you are done with it (the next browser__navigate opens it \
             again).",
            json!({ "type": "object", "properties": {} }),
        ),
    }
}

/// One browser tool as models get it.
pub fn browser_entry(tool: &str) -> ToolEntry {
    let (title, description, schema) = describe(tool);
    ToolEntry {
        api_name: api_name(BROWSER_SERVER, tool),
        kind: ToolKind::Browser {
            tool: tool.to_owned(),
        },
        title: Some(title.into()),
        description: format!("[browser] {description}"),
        schema,
        read_only: browser_tool_reads_only(tool),
        destructive: false,
    }
}

/// All the browser's tools, in the order models get them.
pub fn browser_entries() -> Vec<ToolEntry> {
    BROWSER_TOOLS.iter().map(|t| browser_entry(t)).collect()
}

/// A short label for the usage log: the goal, the URL, what was clicked or typed.
pub fn browser_label(tool: &str, input: &Value) -> Option<String> {
    let s = |k: &str| {
        input
            .get(k)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(|v| v.chars().take(200).collect::<String>())
    };
    match tool {
        BROWSER_TASK => s("goal"),
        BROWSER_NAVIGATE => s("url"),
        BROWSER_SEARCH => s("query"),
        BROWSER_CLICK => input
            .get("ref")
            .and_then(Value::as_u64)
            .map(|r| format!("[{r}]"))
            .or_else(|| s("text")),
        BROWSER_TYPE => s("text").map(|t| format!("“{t}”")),
        BROWSER_READ_PAGE | BROWSER_SCREENSHOT | BROWSER_CLOSE => None,
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_are_namespaced_and_only_reading_is_read_only() {
        let e = browser_entries();
        let names: Vec<&str> = e.iter().map(|x| x.api_name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "browser__browser_task",
                "browser__navigate",
                "browser__search",
                "browser__read_page",
                "browser__click",
                "browser__type",
                "browser__screenshot",
                "browser__close"
            ]
        );
        let ro: Vec<&str> = e.iter().filter(|x| x.read_only).map(|x| x.tool()).collect();
        assert_eq!(ro, vec!["search", "read_page", "screenshot"]);
        let nav = &e[1];
        assert_eq!(nav.server(), Some("browser"));
        assert_eq!(nav.grant_key(), "builtin/browser__navigate");
        assert_eq!(nav.schema["required"], json!(["url"]));
        assert!(nav.description.starts_with("[browser] "));
    }

    #[test]
    fn labels_say_what_the_call_was_about() {
        assert_eq!(
            browser_label(BROWSER_TASK, &json!({"goal": " Find hours "})).as_deref(),
            Some("Find hours")
        );
        assert_eq!(
            browser_label(BROWSER_NAVIGATE, &json!({"url": "https://a.b"})).as_deref(),
            Some("https://a.b")
        );
        assert_eq!(
            browser_label(BROWSER_CLICK, &json!({"ref": 3})).as_deref(),
            Some("[3]")
        );
        assert_eq!(
            browser_label(BROWSER_CLICK, &json!({"text": "Go"})).as_deref(),
            Some("Go")
        );
        assert_eq!(
            browser_label(BROWSER_TYPE, &json!({"text": "rye"})).as_deref(),
            Some("“rye”")
        );
        assert_eq!(
            browser_label(BROWSER_SEARCH, &json!({"query": "museum hours"})).as_deref(),
            Some("museum hours")
        );
        assert_eq!(browser_label(BROWSER_CLOSE, &json!({})), None);
    }
}
