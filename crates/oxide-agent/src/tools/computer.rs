//! The `computer` tool (Phase 16), as models without Anthropic's own computer tool get it:
//! one function with an `action`, the same input as Anthropic's `computer_20251124`.

use oxide_core::computer::{COMPUTER_TOOL, Frame, PANIC_SHORTCUT_LABEL, computer_schema};
use serde_json::Value;

use super::{ToolEntry, ToolKind};

/// The tool, describing the screen it sees.
pub fn computer_entry(frame: &Frame) -> ToolEntry {
    ToolEntry {
        api_name: COMPUTER_TOOL.to_owned(),
        kind: ToolKind::Computer,
        title: Some("Use the computer".into()),
        description: format!(
            "Use the user's computer like a person: see the screen (screenshot), move and click \
             the mouse, type and press keys. The screen is {}×{} pixels in screenshots; \
             coordinates are [x, y] pixels of the latest screenshot from its top-left corner. \
             Take a screenshot first, then act; after each action you get a new screenshot. The \
             user can stop you at any time ({PANIC_SHORTCUT_LABEL}).",
            frame.width, frame.height
        ),
        schema: computer_schema(),
        read_only: false,
        destructive: false,
    }
}

/// A short label for the usage log: the action ("Clicking at (512, 300)").
pub fn computer_label(input: &Value) -> Option<String> {
    oxide_core::computer::parse_action(input)
        .ok()
        .map(|a| a.label())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_entry_names_the_screen_size() {
        let frame = Frame::new(crate::computer::testing::screen());
        let e = computer_entry(&frame);
        assert_eq!(e.api_name, "computer");
        assert!(e.description.contains("1366×768"), "{}", e.description);
        assert_eq!(e.grant_key(), "builtin/computer");
        assert_eq!(e.server(), Some("computer"));
        assert_eq!(
            computer_label(&json!({"action": "key", "text": "ctrl+s"})).as_deref(),
            Some("Pressing ctrl+s")
        );
    }
}
