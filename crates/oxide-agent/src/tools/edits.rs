//! Reading edit blocks from a tool call's arguments (Phase 26), shared by `edit_file`,
//! `app_edit_file` and the `artifact` tool's `update`. The edits themselves are applied by
//! [`oxide_core::edit::apply_edits`].
//!
//! Models write these arguments in many shapes, so the reader is forgiving: an `edits` list
//! (also when sent as a JSON string), or a single edit at the top level; the text to replace
//! as `old_str`, `old_text`, `old_string` or `old`, its replacement as `new_str`,
//! `new_text`, `new_string` or `new`.

use oxide_core::edit::Edit;
use serde_json::{Value, json};

/// Most edits in one call.
pub const MAX_EDITS: usize = 100;

const OLD_KEYS: [&str; 4] = ["old_str", "old_text", "old_string", "old"];
const NEW_KEYS: [&str; 4] = ["new_str", "new_text", "new_string", "new"];

/// The edits in `input`, or a message for the model saying what is missing. `old` and `new`
/// are the names the tool documents, used in the messages.
pub fn parse_edits(input: &Value, old: &str, new: &str) -> Result<Vec<Edit>, String> {
    let list = match input.get("edits") {
        Some(Value::Array(a)) => Some(a.clone()),
        // Some models send the list as a JSON string.
        Some(Value::String(s)) if s.trim_start().starts_with('[') => {
            match serde_json::from_str::<Value>(s) {
                Ok(Value::Array(a)) => Some(a),
                _ => {
                    return Err(format!(
                        "`edits` must be a list of {{\"{old}\", \"{new}\"}} objects."
                    ));
                }
            }
        }
        Some(Value::Null) | None => None,
        Some(_) => {
            return Err(format!(
                "`edits` must be a list of {{\"{old}\", \"{new}\"}} objects."
            ));
        }
    };
    let items = match list {
        Some(items) if !items.is_empty() => items,
        Some(_) => {
            // An empty list next to a top-level edit still counts as that edit.
            if any_key(input, &OLD_KEYS).is_some() {
                vec![input.clone()]
            } else {
                return Err("`edits` is empty: give at least one edit.".to_owned());
            }
        }
        None => vec![input.clone()],
    };
    if items.len() > MAX_EDITS {
        return Err(format!(
            "Too many edits in one call ({}; at most {MAX_EDITS}). Split them, or write the \
             whole file.",
            items.len()
        ));
    }
    let n = items.len();
    items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let which = if n > 1 {
                format!("Edit {} of {n}: ", i + 1)
            } else {
                String::new()
            };
            let Some(o) = any_key(item, &OLD_KEYS) else {
                return Err(format!(
                    "{which}give \"{old}\" (the exact text to replace)."
                ));
            };
            let Some(o) = o.as_str() else {
                return Err(format!("{which}\"{old}\" must be a string."));
            };
            let w = match any_key(item, &NEW_KEYS) {
                None | Some(Value::Null) => "",
                Some(Value::String(s)) => s.as_str(),
                Some(_) => return Err(format!("{which}\"{new}\" must be a string.")),
            };
            let all = item
                .get("replace_all")
                .or_else(|| item.get("replaceAll"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
            Ok(Edit {
                old: o.to_owned(),
                new: w.to_owned(),
                replace_all: all,
            })
        })
        .collect()
}

/// Whether the arguments hold edits (rather than whole content).
pub fn has_edits(input: &Value) -> bool {
    input.get("edits").is_some_and(|v| !v.is_null()) || any_key(input, &OLD_KEYS).is_some()
}

fn any_key<'a>(v: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    keys.iter().find_map(|k| v.get(*k))
}

/// The JSON schema of an `edits` list whose items use `old` / `new` as their field names.
pub fn edits_schema(old: &str, new: &str, what: &str) -> Value {
    json!({
        "type": "array",
        "minItems": 1,
        "description": format!(
            "The changes, applied in order (each to the result of the one before). Each \
             replaces the exact text `{old}` with `{new}`. Quote enough lines of {what} for \
             `{old}` to be found exactly once, unless replace_all is true."
        ),
        "items": {
            "type": "object",
            "properties": {
                old: {"type": "string", "description": format!("Exact text of {what} to replace, copied from it (with its indentation).")},
                new: {"type": "string", "description": "The text to put in its place (empty to delete it)."},
                "replace_all": {"type": "boolean", "description": "Replace every place it appears (default false)."}
            },
            "required": [old, new]
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_are_read_in_every_shape() {
        let list = json!({"edits": [
            {"old_str": "a", "new_str": "b"},
            {"old_text": "c", "new_text": "d", "replace_all": true},
            {"old": "e"}
        ]});
        let e = parse_edits(&list, "old_str", "new_str").expect("parses");
        assert_eq!(e.len(), 3);
        assert_eq!(e[1], Edit::all("c", "d"));
        assert_eq!(e[2], Edit::new("e", ""));

        let single = json!({"path": "x", "old_str": "a", "new_str": "b"});
        assert_eq!(
            parse_edits(&single, "old_str", "new_str").expect("parses"),
            vec![Edit::new("a", "b")]
        );

        let stringly = json!({"edits": "[{\"old_str\": \"a\", \"new_str\": \"b\"}]"});
        assert_eq!(
            parse_edits(&stringly, "old_str", "new_str").expect("parses"),
            vec![Edit::new("a", "b")]
        );
    }

    #[test]
    fn missing_parts_are_explained() {
        let e = parse_edits(&json!({"edits": [{"new_str": "b"}]}), "old_str", "new_str")
            .expect_err("no old");
        assert!(e.contains("old_str"), "{e}");
        let e = parse_edits(&json!({"edits": []}), "old_str", "new_str").expect_err("empty");
        assert!(e.contains("empty"), "{e}");
        let e = parse_edits(
            &json!({"edits": [{"old_str": "a"}, {"old_str": 3}]}),
            "old_str",
            "new_str",
        )
        .expect_err("bad");
        assert!(e.starts_with("Edit 2 of 2"), "{e}");
        assert!(!has_edits(&json!({"content": "x"})));
        assert!(has_edits(&json!({"old_str": "x"})));
    }
}
