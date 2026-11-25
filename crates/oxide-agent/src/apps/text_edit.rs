//! `oxide.ai.edit` (SDK method `text.edit`, Phase 26): changes one text file with the app's
//! model, the way chats' `edit_file` does — the model answers with edit blocks (exact passage
//! → replacement) as structured output, applied by [`oxide_core::edit::apply_edits`], so only
//! those parts change. A failed edit goes back to the model once with the engine's error (it
//! quotes the closest lines) for corrected blocks; as the last resort the model writes the
//! whole file as plain text, which is refused when the answer was cut off.

use oxide_core::edit::{Edit, EditResult, apply_edits};
use oxide_core::{AppCallError, AppManifest, AppPrefs};
use serde_json::{Value, json};

use super::runtime::{AppRuntime, CallEvents};

/// Output tokens for an answer when the app doesn't say (edit blocks, or a whole file).
const DEFAULT_MAX_TOKENS: u64 = 16_384;

const EDIT_RULES: &str = "You change one existing file with edit blocks. Answer with JSON: \
{\"edits\": [{\"old\": \"…\", \"new\": \"…\"}]}. Rules:\n\
- `old` is a passage copied exactly from the file as it is now, character for character, \
with its indentation. Include enough unchanged lines around the change (usually 2–4) that it \
appears exactly once in the file. Never add line numbers.\n\
- `new` is the complete text that replaces it: no placeholders such as \"...\" or \"rest \
unchanged\".\n\
- Edits apply in order, each to the result of the one before; they must not overlap.\n\
- Keep each edit small: the lines that change plus the context needed to find them. Several \
small edits are better than one large one.\n\
- To add something, quote the lines next to where it goes and repeat them in `new` with the \
addition. To delete, give an empty `new`.\n\
- Set \"replace_all\": true only to change every place a passage appears.\n\
- If this file needs no change, answer {\"edits\": []}.";

fn edit_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "edits": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "old": {"type": "string"},
                        "new": {"type": "string"},
                        "replace_all": {"type": "boolean"}
                    },
                    "required": ["old", "new"]
                }
            }
        },
        "required": ["edits"]
    })
}

/// How the change was made.
enum Outcome {
    Edited {
        done: EditResult,
        edits: Vec<Edit>,
        retried: bool,
    },
    NoChange,
    Rewritten {
        content: String,
        reason: String,
    },
}

impl AppRuntime {
    pub(super) async fn text_edit(
        &self,
        manifest: &AppManifest,
        prefs: &AppPrefs,
        p: &Value,
        events: CallEvents,
    ) -> Result<Value, AppCallError> {
        let content = p
            .get("content")
            .and_then(Value::as_str)
            .ok_or_else(|| AppCallError::failed("ai.edit needs the file's `content`"))?
            .to_owned();
        let request = p
            .get("request")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| AppCallError::failed("ai.edit needs a `request`"))?
            .to_owned();
        let path = p
            .get("path")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("the file")
            .to_owned();
        let force_rewrite = p.get("mode").and_then(Value::as_str) == Some("rewrite");
        let stage = |name: &str, note: Option<&str>| {
            events(json!({ "type": "stage", "stage": name, "note": note }));
        };

        let user_system = p
            .get("system")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_owned();
        let context = p
            .get("context")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let mut prompt = String::new();
        if let Some(c) = context {
            prompt.push_str(c);
            prompt.push_str("\n\n");
        }
        prompt.push_str(&format!(
            "## The change to make\n{request}\n\n## {path} as it is now\n<file path=\"{path}\">\n\
             {content}\n</file>"
        ));
        let max_tokens = p
            .get("maxTokens")
            .and_then(Value::as_u64)
            .unwrap_or(DEFAULT_MAX_TOKENS);
        let media = |mut v: Value| {
            for key in ["images", "documents"] {
                if let Some(m) = p.get(key) {
                    v[key] = m.clone();
                }
            }
            v
        };
        let join = |a: &str, b: &str| {
            if a.is_empty() {
                b.to_owned()
            } else {
                format!("{a}\n\n{b}")
            }
        };

        let mut model = None;
        let outcome = 'outcome: {
            if force_rewrite || content.trim().is_empty() {
                break 'outcome None;
            }
            stage("edits", None);
            let quiet: CallEvents = std::sync::Arc::new(|_| {});
            let ask = |messages: Value| {
                media(json!({
                    "messages": messages,
                    "system": join(&user_system, EDIT_RULES),
                    "json": edit_schema(),
                    "maxTokens": max_tokens,
                }))
            };
            let first_messages = json!([{ "role": "user", "content": prompt }]);
            let first = match self
                .text(manifest, prefs, &ask(first_messages.clone()), quiet.clone())
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    break 'outcome Some(Err(format!(
                        "the edit blocks couldn't be read ({})",
                        e.message
                    )));
                }
            };
            model = first.get("model").cloned();
            let edits = match edits_of(&first["json"]) {
                Ok(e) => e,
                Err(e) => break 'outcome Some(Err(e)),
            };
            if edits.is_empty() {
                break 'outcome Some(Ok(Outcome::NoChange));
            }
            match apply_edits(&content, &edits) {
                Ok(done) => {
                    break 'outcome Some(Ok(Outcome::Edited {
                        done,
                        edits,
                        retried: false,
                    }));
                }
                Err(err) => {
                    let why = err.message("old");
                    tracing::info!(path = %path, error = %why, "app edit blocks didn't apply; asking again");
                    stage("retry", Some(&why));
                    let answer = first
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_owned();
                    let retry_messages = json!([
                        { "role": "user", "content": prompt },
                        { "role": "assistant", "content": answer },
                        { "role": "user", "content": format!(
                            "Those edit blocks couldn't be applied, so the file is unchanged: \
                             {why}\nAnswer again with the corrected JSON: all the edit blocks for \
                             the whole change, each `old` copied exactly from the file as given \
                             above."
                        )},
                    ]);
                    let second = match self
                        .text(manifest, prefs, &ask(retry_messages), quiet)
                        .await
                    {
                        Ok(r) => r,
                        Err(e) => {
                            break 'outcome Some(Err(format!(
                                "the corrected edit blocks couldn't be read ({})",
                                e.message
                            )));
                        }
                    };
                    let edits = match edits_of(&second["json"]) {
                        Ok(e) => e,
                        Err(e) => break 'outcome Some(Err(e)),
                    };
                    if edits.is_empty() {
                        break 'outcome Some(Ok(Outcome::NoChange));
                    }
                    match apply_edits(&content, &edits) {
                        Ok(done) => Some(Ok(Outcome::Edited {
                            done,
                            edits,
                            retried: true,
                        })),
                        Err(err) => Some(Err(format!(
                            "the corrected edit blocks didn't apply either: {}",
                            err.message("old")
                        ))),
                    }
                }
            }
        };

        let outcome = match outcome {
            Some(Ok(o)) => o,
            other => {
                let reason = match other {
                    Some(Err(why)) => {
                        tracing::info!(path = %path, reason = %why, "app edit falls back to a rewrite");
                        why
                    }
                    _ if force_rewrite => "a rewrite was asked for".to_owned(),
                    _ => "the file was empty".to_owned(),
                };
                stage("rewrite", Some(&reason));
                let stream = p.get("stream").and_then(Value::as_bool).unwrap_or(false);
                let system = join(
                    &user_system,
                    &format!(
                        "Answer with the complete new content of {path} as plain text: no \
                         Markdown code fences, no explanations before or after. Keep everything \
                         that doesn't need to change exactly as it is."
                    ),
                );
                let r = self
                    .text(
                        manifest,
                        prefs,
                        &media(json!({
                            "prompt": prompt,
                            "system": system,
                            "maxTokens": max_tokens,
                            "stream": stream,
                        })),
                        events.clone(),
                    )
                    .await?;
                model = r.get("model").cloned();
                let raw = r.get("text").and_then(Value::as_str).unwrap_or("");
                if r.get("truncated").and_then(Value::as_bool) == Some(true) {
                    return Err(AppCallError::failed(format!(
                        "The model's rewrite of {path} was cut off at its output limit after {} \
                         characters, so the file wasn't changed. With LM Studio, load the model \
                         with a larger context length (Context Length in its load settings); \
                         otherwise ask for a smaller change or pick a model with a larger \
                         context window.",
                        raw.chars().count()
                    )));
                }
                let text = strip_fences(raw);
                if text.trim().is_empty() {
                    return Err(AppCallError::failed(format!(
                        "The model's rewrite of {path} was empty, so the file wasn't changed."
                    )));
                }
                Outcome::Rewritten {
                    content: keep_final_newline(&content, text),
                    reason,
                }
            }
        };

        Ok(match outcome {
            Outcome::Edited {
                done,
                edits,
                retried,
            } => json!({
                "content": done.text,
                "changed": done.text != content,
                "edits": edits.iter().map(|e| json!({
                    "old": e.old, "new": e.new, "replaceAll": e.replace_all,
                })).collect::<Vec<_>>(),
                "ranges": done.ranges,
                "summary": done.summary(),
                "rewritten": false,
                "retried": retried,
                "model": model,
            }),
            Outcome::NoChange => json!({
                "content": content,
                "changed": false,
                "edits": [],
                "ranges": [],
                "summary": "No change was needed.",
                "rewritten": false,
                "retried": false,
                "model": model,
            }),
            Outcome::Rewritten {
                content: next,
                reason,
            } => json!({
                "changed": next != content,
                "content": next,
                "edits": [],
                "ranges": [],
                "summary": format!("Rewritten in full ({reason})."),
                "rewritten": true,
                "reason": reason,
                "retried": false,
                "model": model,
            }),
        })
    }
}

/// The edit blocks in the model's JSON answer.
fn edits_of(v: &Value) -> Result<Vec<Edit>, String> {
    let list = v
        .get("edits")
        .or_else(|| v.as_array().map(|_| v))
        .ok_or("the answer had no \"edits\" list")?;
    crate::tools::edits::parse_edits(&json!({ "edits": list }), "old", "new").or_else(|e| {
        // An empty list means no change.
        if list.as_array().is_some_and(Vec::is_empty) {
            Ok(Vec::new())
        } else {
            Err(e)
        }
    })
}

/// A whole file from a model: without a code fence around it or a sentence before it.
fn strip_fences(raw: &str) -> &str {
    let t = raw.trim_matches(['\n', '\r']);
    let trimmed = t.trim();
    // "Here is the updated file:" before a fence.
    let body = match trimmed.find("```") {
        Some(i)
            if i > 0 && {
                let lead = trimmed[..i].trim_end();
                !lead.contains('\n') && lead.ends_with(':')
            } =>
        {
            &trimmed[i..]
        }
        _ => trimmed,
    };
    if let Some(rest) = body.strip_prefix("```")
        && let Some(nl) = rest.find('\n')
    {
        let inner = &rest[nl + 1..];
        let inner = inner.trim_end();
        if let Some(inner) = inner.strip_suffix("```") {
            return inner
                .strip_suffix('\n')
                .unwrap_or(inner)
                .trim_end_matches('\r');
        }
    }
    t
}

/// The new file ends with a line break when the old one did.
fn keep_final_newline(old: &str, new: &str) -> String {
    let eol = if old.contains("\r\n") { "\r\n" } else { "\n" };
    if old.ends_with('\n') && !new.ends_with('\n') {
        format!("{new}{eol}")
    } else {
        new.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fences_and_lead_in_sentences_are_stripped() {
        assert_eq!(
            strip_fences("```php\n<?php\necho 1;\n```"),
            "<?php\necho 1;"
        );
        assert_eq!(
            strip_fences("Here is the updated file:\n```\na\nb\n```\n"),
            "a\nb"
        );
        assert_eq!(strip_fences("<?php\necho 1;\n"), "<?php\necho 1;");
        // A fence inside the file stays.
        assert_eq!(strip_fences("# A\n```\nx\n```\n"), "# A\n```\nx\n```");
        assert_eq!(keep_final_newline("a\r\n", "b"), "b\r\n");
        assert_eq!(keep_final_newline("a", "b"), "b");
    }

    #[test]
    fn edit_blocks_are_read_from_the_answer() {
        assert_eq!(
            edits_of(&json!({"edits": [{"old": "a", "new": "b"}]})).expect("edits"),
            vec![Edit::new("a", "b")]
        );
        assert!(edits_of(&json!({"edits": []})).expect("none").is_empty());
        assert!(edits_of(&json!({"files": []})).is_err());
    }
}
