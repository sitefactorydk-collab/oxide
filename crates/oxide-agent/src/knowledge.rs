//! Project knowledge: reading files into text, deciding whether a project's knowledge goes
//! into the prompt in full or is searched, and the `project_search` tool.

use oxide_core::project::knowledge_mode;
use oxide_core::tools::PROJECT_SEARCH;
use oxide_core::{
    AttachmentKind, AttachmentSource, KnowledgeHit, KnowledgeKind, KnowledgeMode, KnowledgeText,
};
use oxide_llm::media::pdf_text_sidecar;
use oxide_store::{NewKnowledgeFile, Store};
use serde_json::{Value, json};

use crate::AgentError;
use crate::attachments::AttachmentStore;
use crate::tools::{Builtin, ToolEntry, ToolKind, ToolOutput};

/// Passages `project_search` returns unless the model asks for another number.
pub const DEFAULT_SEARCH_RESULTS: u32 = 6;

/// Reads a file the user adds to a project and returns its text. Text and code files are
/// read as UTF-8, PDFs through their extracted text; images aren't accepted.
pub async fn read_knowledge_file(
    attachments: &AttachmentStore,
    source: AttachmentSource,
) -> Result<NewKnowledgeFile, AgentError> {
    let att = attachments.prepare(source).await?;
    let stored = att.path.clone();
    let result = match att.kind {
        AttachmentKind::Image => Err(AgentError::Invalid(format!(
            "{} is an image. Project knowledge takes text, code and PDF files.",
            att.name
        ))),
        AttachmentKind::Text => tokio::fs::read_to_string(&stored)
            .await
            .map_err(|e| AgentError::Invalid(format!("{} couldn't be read: {e}", att.name)))
            .map(|text| NewKnowledgeFile {
                name: att.name.clone(),
                kind: KnowledgeKind::Text,
                media_type: att.media_type.clone(),
                pages: None,
                note: att.note.clone(),
                text,
            }),
        AttachmentKind::Pdf => {
            let sidecar = pdf_text_sidecar(&stored);
            let text = tokio::fs::read_to_string(&sidecar)
                .await
                .unwrap_or_default();
            let _ = tokio::fs::remove_file(&sidecar).await;
            if text.trim().is_empty() {
                Err(AgentError::Invalid(format!(
                    "No text could be read from {} (scanned pages?). Project knowledge needs \
                     text.",
                    att.name
                )))
            } else {
                Ok(NewKnowledgeFile {
                    name: att.name.clone(),
                    kind: KnowledgeKind::Pdf,
                    media_type: att.media_type.clone(),
                    pages: att.pages,
                    note: None,
                    text,
                })
            }
        }
    };
    // The text lives in the database; the prepared copy isn't needed.
    let _ = tokio::fs::remove_file(&stored).await;
    result
}

/// How one chat uses its project's knowledge.
#[derive(Debug, Clone, PartialEq)]
pub enum KnowledgePlan {
    None,
    /// Every file, as a block for `ChatRequest::system_context`.
    Full(String),
    /// Too large: the file names (for the system prompt) and the `project_search` tool.
    Search(Vec<String>),
}

/// The knowledge budget of a chat: the setting, but at most a quarter of the model's
/// context window when that is known.
pub fn chat_budget(setting: u32, context_window: Option<u32>) -> u32 {
    match context_window {
        Some(w) if w > 0 => setting.min(w / 4),
        _ => setting,
    }
}

/// Full when the files fit `budget` (estimated tokens), else search.
pub fn plan_knowledge(files: &[KnowledgeText], budget: u32) -> KnowledgePlan {
    let total: u32 = files
        .iter()
        .map(|f| f.file.tokens)
        .fold(0, u32::saturating_add);
    match knowledge_mode(total, files.len(), budget) {
        KnowledgeMode::Empty => KnowledgePlan::None,
        KnowledgeMode::Full => KnowledgePlan::Full(full_context(files)),
        KnowledgeMode::Search => {
            KnowledgePlan::Search(files.iter().map(|f| f.file.name.clone()).collect())
        }
    }
}

fn escape_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
}

/// The project's files in full, for the start of the prompt.
pub fn full_context(files: &[KnowledgeText]) -> String {
    let mut s = String::from(
        "<project_knowledge>\nThe user added these files to the project this chat belongs to. \
         They are given in full. Use them when they're relevant to the question, and say which \
         file you drew on.\n",
    );
    for (i, f) in files.iter().enumerate() {
        s.push_str(&format!(
            "<document index=\"{}\" name=\"{}\">\n{}\n</document>\n",
            i + 1,
            escape_attr(&f.file.name),
            f.text.trim()
        ));
    }
    s.push_str("</project_knowledge>");
    s
}

/// The `project_search` tool as the model sees it.
pub fn project_search_entry() -> ToolEntry {
    ToolEntry {
        api_name: PROJECT_SEARCH.into(),
        kind: ToolKind::Builtin(Builtin::ProjectSearch),
        title: Some("Search project knowledge".into()),
        description: "Search the files in this chat's project knowledge. Returns the most \
                      relevant passages (ranked by BM25 keyword relevance) with their file \
                      names. Use specific keywords from the question; search again with other \
                      words if the first results don't answer it."
            .into(),
        schema: json!({
            "type": "object",
            "properties": {
                "query": { "type": "string",
                           "description": "Keywords or a question to search for." },
                "max_results": { "type": "integer", "minimum": 1, "maximum": 20,
                                 "description": "How many passages (default 6)." }
            },
            "required": ["query"]
        }),
        read_only: true,
        destructive: false,
    }
}

/// Runs `project_search` in `project_id`.
pub async fn run_project_search(
    store: &Store,
    project_id: Option<&str>,
    input: &Value,
) -> ToolOutput {
    let Some(project_id) = project_id else {
        return ToolOutput::error("This chat isn't in a project.");
    };
    let Some(query) = input
        .get("query")
        .and_then(Value::as_str)
        .filter(|q| !q.trim().is_empty())
    else {
        return ToolOutput::error("project_search needs a \"query\" string.");
    };
    let max = input
        .get("max_results")
        .and_then(Value::as_u64)
        .map(|n| u32::try_from(n).unwrap_or(20))
        .unwrap_or(DEFAULT_SEARCH_RESULTS)
        .clamp(1, 20);
    match store.search_knowledge(project_id, query, max).await {
        Ok(hits) => ToolOutput {
            content: vec![oxide_core::ContentBlock::text(format_hits(query, &hits))],
            is_error: false,
            duration_ms: 0,
        },
        Err(e) => ToolOutput::error(format!("The search failed: {e}")),
    }
}

/// Search results as text for the model.
pub fn format_hits(query: &str, hits: &[KnowledgeHit]) -> String {
    if hits.is_empty() {
        return format!(
            "No passages in the project knowledge match “{query}”. Try other keywords, or \
             answer without the files and say they didn't cover it."
        );
    }
    let mut s = format!("{} passages for “{query}”, best first:\n", hits.len());
    for (i, h) in hits.iter().enumerate() {
        s.push_str(&format!(
            "\n<passage index=\"{}\" file=\"{}\" part=\"{}\">\n{}\n</passage>\n",
            i + 1,
            escape_attr(&h.file_name),
            h.chunk + 1,
            h.text.trim()
        ));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxide_core::KnowledgeFile;
    use oxide_core::project::estimate_tokens;

    fn text(name: &str, body: &str) -> KnowledgeText {
        KnowledgeText {
            file: KnowledgeFile {
                id: name.into(),
                project_id: "p".into(),
                name: name.into(),
                kind: KnowledgeKind::Text,
                media_type: "text/plain".into(),
                chars: u32::try_from(body.chars().count()).unwrap_or(0),
                tokens: estimate_tokens(body.chars().count()),
                pages: None,
                note: None,
                created_at: 0,
            },
            text: body.into(),
        }
    }

    #[test]
    fn small_projects_go_in_full_and_large_ones_are_searched() {
        assert_eq!(plan_knowledge(&[], 100), KnowledgePlan::None);
        let files = vec![text("a \"x\".md", "alpha"), text("b.txt", "beta")];
        let KnowledgePlan::Full(block) = plan_knowledge(&files, 100) else {
            panic!("fits");
        };
        assert!(block.starts_with("<project_knowledge>"));
        assert!(block.contains("<document index=\"1\" name=\"a &quot;x&quot;.md\">\nalpha\n"));
        assert!(block.contains("<document index=\"2\" name=\"b.txt\">\nbeta\n"));
        let big = vec![
            text("big.md", &"word ".repeat(1_000)),
            text("b.txt", "beta"),
        ];
        assert_eq!(
            plan_knowledge(&big, 1_000),
            KnowledgePlan::Search(vec!["big.md".into(), "b.txt".into()])
        );
        assert_eq!(chat_budget(40_000, Some(32_000)), 8_000);
        assert_eq!(chat_budget(40_000, None), 40_000);
        assert_eq!(chat_budget(40_000, Some(1_000_000)), 40_000);
    }

    #[test]
    fn hits_are_formatted_with_files() {
        let none = format_hits("cats", &[]);
        assert!(none.contains("No passages"));
        let s = format_hits(
            "cats",
            &[KnowledgeHit {
                file_id: "f".into(),
                file_name: "pets.md".into(),
                chunk: 2,
                start: 10,
                text: " Cats sleep a lot. ".into(),
                score: 1.0,
            }],
        );
        assert!(
            s.contains("<passage index=\"1\" file=\"pets.md\" part=\"3\">\nCats sleep a lot.\n")
        );
    }

    #[tokio::test]
    async fn files_are_read_into_text() {
        let dir = tempfile::tempdir().expect("dir");
        let store = AttachmentStore::new(dir.path().join("att"));
        let bytes = |name: &str, data: &[u8]| AttachmentSource::Bytes {
            name: name.into(),
            data: base64::Engine::encode(&base64::engine::general_purpose::STANDARD, data),
        };
        let f = read_knowledge_file(&store, bytes("notes.md", b"# Notes\nhello"))
            .await
            .expect("reads");
        assert_eq!(f.kind, KnowledgeKind::Text);
        assert_eq!(f.text, "# Notes\nhello");
        let pdf = oxide_llm::media::sample_pdf(&["Page one text", "Page two"]);
        let f = read_knowledge_file(&store, bytes("doc.pdf", &pdf))
            .await
            .expect("reads");
        assert_eq!(f.kind, KnowledgeKind::Pdf);
        assert_eq!(f.pages, Some(2));
        assert!(f.text.contains("Page one text"), "{}", f.text);
        // Nothing is left behind in the attachments folder.
        let left = std::fs::read_dir(dir.path().join("att"))
            .expect("dir")
            .count();
        assert_eq!(left, 0);
        // A 1×1 PNG.
        let png = {
            let img = image::RgbImage::new(1, 1);
            let mut out = std::io::Cursor::new(Vec::new());
            image::DynamicImage::ImageRgb8(img)
                .write_to(&mut out, image::ImageFormat::Png)
                .expect("encodes");
            out.into_inner()
        };
        assert!(matches!(
            read_knowledge_file(&store, bytes("a.png", &png)).await,
            Err(AgentError::Invalid(_))
        ));
    }
}
