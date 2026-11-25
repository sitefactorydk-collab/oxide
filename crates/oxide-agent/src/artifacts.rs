//! The built-in `artifact` tool: the model creates and changes artifacts (documents, code,
//! web pages, SVG, Mermaid diagrams, React components), which the app shows in a side panel.
//! Every call that changes something saves a new version. It runs in the chat service (it
//! needs the chat and the reply) and never asks for approval: it only writes into Oxide.

use oxide_core::artifact::{
    ARTIFACT_TOOL, MAX_ARTIFACT_CHARS, clean_key, clean_title, key_from_title,
};
use oxide_core::{ArtifactKind, ContentBlock};
use oxide_store::{NewArtifactVersion, Store};
use serde_json::{Value, json};

use crate::tools::{Builtin, ToolEntry, ToolKind, ToolOutput};

const DESCRIPTION: &str = "Create or change an artifact: substantial, self-contained content \
shown in a panel beside the chat, where the user can view, copy and download it and switch \
between versions. Use it for content the user is likely to keep, reuse, edit or run: documents \
and reports (markdown), code files and scripts longer than about 15 lines (code), web pages \
(html), SVG images (svg), diagrams (mermaid) and interactive components or small apps (react). \
Don't use it for short snippets, brief answers, or content that only makes sense inline in the \
conversation. Make one artifact per distinct piece of content. To change an artifact, call the \
tool again with the same id: command \"update\" changes it with edit blocks (\"edits\": each \
replaces an exact passage old_str, copied from the current content with enough lines to be found \
once, with new_str; several apply in order, all or nothing; the rest stays the same), \
\"rewrite\" replaces the whole content (only when most of it changes). After a call, tell the user briefly what you made or changed; don't repeat the content \
in your reply. Notes by type: html is a complete page with inline styles and scripts (scripts \
from https CDNs work; there are no network requests otherwise). react is one component file \
with a default export and no required props; it may import only from \"react\" and \
\"lucide-react\", is styled with Tailwind classes, and runs offline in a sandbox (no fetch, no \
localStorage, no forms that submit). svg is a single <svg> element with a viewBox. mermaid is \
the diagram source only, without code fences. code needs language.";

/// The tool as offered to the model.
pub fn artifact_entry() -> ToolEntry {
    ToolEntry {
        api_name: ARTIFACT_TOOL.into(),
        kind: ToolKind::Builtin(Builtin::Artifact),
        title: Some("Artifact".into()),
        description: DESCRIPTION.into(),
        // Property order matters: models write arguments in this order, and the panel can
        // show the title and type while the content streams.
        schema: json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "enum": ["create", "update", "rewrite"],
                             "description": "create a new artifact, update parts of one with \
                                             edit blocks, or rewrite all of it." },
                "id": { "type": "string",
                        "description": "A short kebab-case id you choose, e.g. \"todo-app\". \
                                        Use the same id to change the artifact later." },
                "type": { "type": "string",
                          "enum": ["markdown", "code", "html", "svg", "mermaid", "react"] },
                "title": { "type": "string", "description": "A short, descriptive title." },
                "language": { "type": "string",
                              "description": "For type code: the language, e.g. python." },
                "content": { "type": "string",
                             "description": "create and rewrite: the complete content." },
                "edits": crate::tools::edits::edits_schema("old_str", "new_str", "the current content")
            },
            "required": ["command", "id"]
        }),
        read_only: false,
        destructive: false,
    }
}

fn str_arg<'a>(input: &'a Value, key: &str) -> Option<&'a str> {
    input.get(key).and_then(Value::as_str)
}

fn non_empty<'a>(input: &'a Value, key: &str) -> Option<&'a str> {
    str_arg(input, key).filter(|s| !s.trim().is_empty())
}

/// Strips one surrounding Markdown code fence (models sometimes wrap content in one).
fn unfence(content: &str, kind: ArtifactKind) -> String {
    if kind == ArtifactKind::Markdown {
        return content.to_owned();
    }
    let t = content.trim();
    if let Some(rest) = t.strip_prefix("```")
        && let Some(body) = rest.strip_suffix("```")
        && let Some(nl) = body.find('\n')
        && !body[..nl].contains(' ')
    {
        return body[nl + 1..].trim_end_matches(['\n', '\r']).to_owned();
    }
    content.to_owned()
}

/// Runs one `artifact` call for chat `conversation_id`, made by reply `message_id`.
pub async fn run_artifact(
    store: &Store,
    conversation_id: &str,
    message_id: &str,
    input: &Value,
) -> ToolOutput {
    let command = non_empty(input, "command").map(|c| c.trim().to_ascii_lowercase());
    let title_arg = non_empty(input, "title").and_then(clean_title);
    let key = match non_empty(input, "id").and_then(clean_key) {
        Some(k) => k,
        None => match &title_arg {
            Some(t) => key_from_title(t),
            None => return ToolOutput::error("artifact needs an \"id\"."),
        },
    };
    let kind_arg = match non_empty(input, "type") {
        Some(t) => match ArtifactKind::parse(t) {
            Some(k) => Some(k),
            None => {
                return ToolOutput::error(format!(
                    "Unknown artifact type “{t}”. Use markdown, code, html, svg, mermaid or react."
                ));
            }
        },
        None => None,
    };
    let language_arg = non_empty(input, "language").map(|l| l.trim().to_owned());

    let existing = match store.artifact_by_key(conversation_id, &key).await {
        Ok(e) => e,
        Err(e) => return ToolOutput::error(format!("The artifact couldn't be read: {e}")),
    };
    let command = match command.as_deref() {
        Some(c @ ("create" | "update" | "rewrite")) => c.to_owned(),
        Some(other) => {
            return ToolOutput::error(format!(
                "Unknown command “{other}”. Use create, update or rewrite."
            ));
        }
        // Guess from the arguments.
        None if crate::tools::edits::has_edits(input) => "update".into(),
        None if existing.is_some() => "rewrite".into(),
        None => "create".into(),
    };

    let mut update_note: Option<String> = None;
    let (kind, title, language, content) = match (command.as_str(), &existing) {
        ("update", None) => {
            return ToolOutput::error(format!(
                "There's no artifact with id “{key}” in this chat. Create it first."
            ));
        }
        ("update", Some((a, current))) => {
            let edits = match crate::tools::edits::parse_edits(input, "old_str", "new_str") {
                Ok(e) => e,
                Err(e) => return ToolOutput::error(format!("update: {e}")),
            };
            let content = match oxide_core::edit::apply_edits(current, &edits) {
                Ok(done) => {
                    update_note = Some(done.summary());
                    done.text
                }
                Err(e) => {
                    return ToolOutput::error(format!(
                        "The update wasn't applied: {} Copy old_str exactly from the current \
                         content, or use \"rewrite\" with the whole new content if most of it \
                         changes.",
                        e.message("old_str")
                    ));
                }
            };
            (
                kind_arg.unwrap_or(a.kind),
                title_arg.unwrap_or_else(|| a.title.clone()),
                language_arg.or_else(|| a.language.clone()),
                content,
            )
        }
        // create and rewrite: the whole content.
        (_, existing) => {
            let Some(content) = str_arg(input, "content") else {
                return ToolOutput::error(format!("{command} needs \"content\"."));
            };
            let prev = existing.as_ref().map(|(a, _)| a);
            // A new artifact without a type takes it from its language (`"language":
            // "markdown"` is a common slip): a known type by name, else code.
            let from_language = || {
                language_arg
                    .as_deref()
                    .map(|l| ArtifactKind::parse(l).unwrap_or(ArtifactKind::Code))
            };
            let Some(kind) = kind_arg.or(prev.map(|a| a.kind)).or_else(from_language) else {
                return ToolOutput::error(
                    "A new artifact needs a \"type\": markdown, code, html, svg, mermaid or react.",
                );
            };
            let title = title_arg
                .or_else(|| prev.map(|a| a.title.clone()))
                .unwrap_or_else(|| key.clone());
            let language = language_arg.or_else(|| prev.and_then(|a| a.language.clone()));
            (kind, title, language, unfence(content, kind))
        }
    };
    if content.trim().is_empty() {
        return ToolOutput::error("The artifact's content is empty.");
    }
    if content.chars().count() > MAX_ARTIFACT_CHARS {
        return ToolOutput::error(format!(
            "The artifact is too long (the limit is {MAX_ARTIFACT_CHARS} characters). Split it \
             into several artifacts."
        ));
    }
    let language = if kind == ArtifactKind::Code {
        language
    } else {
        None
    };
    let saved = store
        .save_artifact_version(NewArtifactVersion {
            conversation_id: conversation_id.to_owned(),
            key: key.clone(),
            kind,
            title: title.clone(),
            language,
            content,
            message_id: Some(message_id.to_owned()),
        })
        .await;
    match saved {
        Ok((artifact, version)) => {
            let verb = match (existing.is_some(), command.as_str()) {
                (false, _) => "Created",
                (true, "update") => "Updated",
                (true, _) => "Rewrote",
            };
            let lines = version.content.lines().count();
            ToolOutput {
                content: vec![
                    ContentBlock::text(format!(
                        "{verb} artifact “{}” (id \"{key}\", type {}, version {}, {lines} lines). \
                         The user sees it in the panel beside the chat.{}",
                        artifact.title,
                        artifact.kind.as_str(),
                        artifact.version,
                        update_note.map(|n| format!(" {n}")).unwrap_or_default()
                    )),
                    ContentBlock::ArtifactRef {
                        artifact_id: artifact.id,
                        title: artifact.title,
                        version: artifact.version,
                    },
                ],
                is_error: false,
                duration_ms: 0,
            }
        }
        Err(e) => ToolOutput::error(format!("The artifact couldn't be saved: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxide_core::StartConversation;

    async fn chat() -> (Store, String) {
        let store = Store::open_in_memory().await.expect("store");
        let (c, _) = store
            .start_conversation(StartConversation {
                text: "hi".into(),
                model: "anthropic/x".into(),
                project_id: None,
                attachments: Vec::new(),
                folders: vec![],
                tools: None,
            })
            .await
            .expect("chat");
        (store, c.id)
    }

    fn reference(out: &ToolOutput) -> (String, u32) {
        out.content
            .iter()
            .find_map(|b| match b {
                ContentBlock::ArtifactRef {
                    artifact_id,
                    version,
                    ..
                } => Some((artifact_id.clone(), *version)),
                _ => None,
            })
            .expect("an artifact reference")
    }

    #[tokio::test]
    async fn a_missing_type_comes_from_the_language() {
        let (s, cid) = chat().await;
        let out = run_artifact(
            &s,
            &cid,
            "r1",
            &json!({"command": "create", "id": "notes", "language": "markdown",
                    "title": "Notes", "content": "# Hi"}),
        )
        .await;
        assert!(!out.is_error, "{out:?}");
        let text = oxide_core::message::plain_text(&out.content);
        assert!(text.contains("type markdown"), "{text}");
        let out = run_artifact(
            &s,
            &cid,
            "r2",
            &json!({"command": "create", "id": "script", "language": "python",
                    "content": "print(1)"}),
        )
        .await;
        assert!(!out.is_error, "{out:?}");
        assert!(oxide_core::message::plain_text(&out.content).contains("type code"));
        // Neither: still an error.
        let out = run_artifact(
            &s,
            &cid,
            "r3",
            &json!({"command": "create", "id": "x", "content": "?"}),
        )
        .await;
        assert!(out.is_error);
    }

    #[tokio::test]
    async fn create_update_and_rewrite_make_versions() {
        let (s, cid) = chat().await;
        let out = run_artifact(
            &s,
            &cid,
            "r1",
            &json!({"command": "create", "id": "Hello App", "type": "react",
                    "title": "Hello", "content": "export default () => <p>Hi</p>;"}),
        )
        .await;
        assert!(!out.is_error, "{out:?}");
        let (id, v) = reference(&out);
        assert_eq!(v, 1);
        assert!(oxide_core::message::plain_text(&out.content).contains("id \"hello-app\""));

        let out = run_artifact(
            &s,
            &cid,
            "r2",
            &json!({"command": "update", "id": "hello-app", "old_str": "Hi", "new_str": "Hello"}),
        )
        .await;
        assert!(!out.is_error, "{out:?}");
        assert_eq!(reference(&out), (id.clone(), 2));
        let v2 = s.artifact_version(&id, Some(2)).await.expect("v2");
        assert_eq!(v2.content, "export default () => <p>Hello</p>;");
        assert_eq!(v2.kind, ArtifactKind::React);
        assert_eq!(v2.message_id.as_deref(), Some("r2"));

        let out = run_artifact(
            &s,
            &cid,
            "r3",
            &json!({"command": "rewrite", "id": "hello-app", "title": "Greeting",
                    "content": "```jsx\nexport default () => null;\n```"}),
        )
        .await;
        assert_eq!(reference(&out).1, 3);
        let v3 = s.artifact_version(&id, None).await.expect("v3");
        // The fence is stripped; the type carries over.
        assert_eq!(v3.content, "export default () => null;");
        assert_eq!(v3.title, "Greeting");
        assert_eq!(v3.kind, ArtifactKind::React);

        // Edit blocks: several in one update, then a misquoted one that suggests rewrite.
        let out = run_artifact(
            &s,
            &cid,
            "r4",
            &json!({"command": "rewrite", "id": "hello-app",
                    "content": "const a = 1;\nconst b = 2;\nexport default () => null;"}),
        )
        .await;
        assert!(!out.is_error, "{out:?}");
        let out = run_artifact(
            &s,
            &cid,
            "r5",
            &json!({"id": "hello-app", "edits": [
                {"old_str": "const a = 1;", "new_str": "const a = 10;"},
                {"old_str": "const b = 2;", "new_str": "const b = 20;"}
            ]}),
        )
        .await;
        assert!(!out.is_error, "{out:?}");
        let said = oxide_core::message::plain_text(&out.content);
        assert!(said.starts_with("Updated"), "{said}");
        assert!(said.contains("2 edits applied: lines 1–2."), "{said}");
        let v5 = s.artifact_version(&id, None).await.expect("v5");
        assert_eq!(
            v5.content,
            "const a = 10;\nconst b = 20;\nexport default () => null;"
        );
        let out = run_artifact(
            &s,
            &cid,
            "r6",
            &json!({"command": "update", "id": "hello-app", "edits": [
                {"old_str": "const c = 20;", "new_str": "const c = 3;"}
            ]}),
        )
        .await;
        assert!(out.is_error);
        let said = oxide_core::message::plain_text(&out.content);
        assert!(said.contains("const b = 20;"), "{said}");
        assert!(said.contains("rewrite"), "{said}");
    }

    #[tokio::test]
    async fn mistakes_come_back_as_errors_the_model_can_fix() {
        let (s, cid) = chat().await;
        let err = |o: ToolOutput| {
            assert!(o.is_error);
            oxide_core::message::plain_text(&o.content)
        };
        let e = err(run_artifact(
            &s,
            &cid,
            "r",
            &json!({"command": "update", "id": "x",
            "old_str": "a", "new_str": "b"}),
        )
        .await);
        assert!(e.contains("no artifact"), "{e}");
        let e = err(run_artifact(
            &s,
            &cid,
            "r",
            &json!({"command": "create", "id": "x",
            "content": "c"}),
        )
        .await);
        assert!(e.contains("type"), "{e}");
        let e = err(run_artifact(
            &s,
            &cid,
            "r",
            &json!({"command": "create", "id": "x",
            "type": "spreadsheet", "content": "c"}),
        )
        .await);
        assert!(e.contains("Unknown artifact type"), "{e}");
        let e = err(run_artifact(
            &s,
            &cid,
            "r",
            &json!({"command": "create", "type": "code",
            "content": "c"}),
        )
        .await);
        assert!(e.contains("id"), "{e}");

        let ok = run_artifact(
            &s,
            &cid,
            "r",
            &json!({"id": "notes", "type": "markdown",
            "content": "a a"}),
        )
        .await;
        assert!(!ok.is_error);
        let e = err(run_artifact(
            &s,
            &cid,
            "r",
            &json!({"command": "update", "id": "notes",
            "old_str": "a", "new_str": "b"}),
        )
        .await);
        assert!(e.contains("found 2 times"), "{e}");
        // Code keeps its language; other types don't store one.
        let code = run_artifact(
            &s,
            &cid,
            "r",
            &json!({"command": "create", "id": "main",
            "type": "code", "language": "rust", "title": "main.rs", "content": "fn main() {}"}),
        )
        .await;
        let (id, _) = reference(&code);
        assert_eq!(
            s.artifact_version(&id, None)
                .await
                .expect("v")
                .language
                .as_deref(),
            Some("rust")
        );
    }
}
