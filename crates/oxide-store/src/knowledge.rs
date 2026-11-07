//! Project knowledge: the text of each file, cut into passages that SQLite FTS5 indexes, so
//! `project_search` can rank them with BM25.

use oxide_core::project::estimate_tokens;
use oxide_core::{KnowledgeFile, KnowledgeHit, KnowledgeKind, KnowledgeText};

use crate::conversations::map_fk;
use crate::{Result, Store, StoreError, new_id, now_ms};

/// Target passage length, in characters.
pub const CHUNK_CHARS: usize = 1_500;
/// Characters a passage repeats from the end of the one before it.
pub const CHUNK_OVERLAP: usize = 200;
/// Most knowledge files in one project.
pub const MAX_PROJECT_FILES: i64 = 500;
/// Most characters of one knowledge file.
pub const MAX_FILE_CHARS: usize = 2_000_000;

/// A file to add to a project (its text already extracted).
#[derive(Debug, Clone)]
pub struct NewKnowledgeFile {
    pub name: String,
    pub kind: KnowledgeKind,
    pub media_type: String,
    pub pages: Option<u32>,
    pub note: Option<String>,
    pub text: String,
}

#[derive(sqlx::FromRow)]
struct FileRow {
    id: String,
    project_id: String,
    name: String,
    kind: String,
    media_type: String,
    chars: i64,
    pages: Option<i64>,
    note: Option<String>,
    created_at: i64,
}

impl From<FileRow> for KnowledgeFile {
    fn from(r: FileRow) -> Self {
        let chars = u32::try_from(r.chars).unwrap_or(u32::MAX);
        KnowledgeFile {
            id: r.id,
            project_id: r.project_id,
            name: r.name,
            kind: KnowledgeKind::parse(&r.kind),
            media_type: r.media_type,
            chars,
            tokens: estimate_tokens(chars as usize),
            pages: r.pages.and_then(|p| u32::try_from(p).ok()),
            note: r.note,
            created_at: r.created_at,
        }
    }
}

const FILE_COLUMNS: &str = "id, project_id, name, kind, media_type, chars, pages, note, created_at";

const NOT_FOUND: StoreError = StoreError::NotFound {
    what: "knowledge file",
};

/// Cuts `text` into passages of about [`CHUNK_CHARS`] characters that overlap by about
/// [`CHUNK_OVERLAP`], breaking at paragraphs, lines, sentences or spaces where possible.
/// Returns each passage's character offset and text (blank passages are left out).
pub fn chunk_text(text: &str) -> Vec<(usize, String)> {
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let mut out = Vec::new();
    let mut start = 0usize;
    while start < n {
        let hard_end = (start + CHUNK_CHARS).min(n);
        let end = if hard_end == n {
            n
        } else {
            break_point(&chars, start + CHUNK_CHARS / 2, hard_end).unwrap_or(hard_end)
        };
        let piece: String = chars[start..end].iter().collect();
        if !piece.trim().is_empty() {
            out.push((start, piece.trim().to_owned()));
        }
        if end >= n {
            break;
        }
        // Step back for the overlap, to the start of a word if one is near.
        let back = end.saturating_sub(CHUNK_OVERLAP).max(start + 1);
        start = (back..end.min(back + 64))
            .find(|&i| chars[i - 1].is_whitespace())
            .unwrap_or(back);
    }
    out
}

/// The best place to end a passage in `chars[from..to]`: after a blank line, a line break,
/// a sentence end or a space (latest of the best kind).
fn break_point(chars: &[char], from: usize, to: usize) -> Option<usize> {
    let window = from..to;
    let last = |pred: &dyn Fn(usize) -> bool| window.clone().rev().find(|&i| pred(i));
    last(&|i| i >= 1 && chars[i - 1] == '\n' && chars[i] == '\n')
        .map(|i| i + 1)
        .or_else(|| last(&|i| chars[i] == '\n').map(|i| i + 1))
        .or_else(|| {
            last(&|i| i >= 1 && matches!(chars[i - 1], '.' | '!' | '?') && chars[i] == ' ')
                .map(|i| i + 1)
        })
        .or_else(|| last(&|i| chars[i].is_whitespace()).map(|i| i + 1))
}

/// Words too common to help a search (English and Danish). Left out of queries unless the
/// query has nothing else.
const STOP_WORDS: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "by", "can", "do", "does", "for", "from", "how",
    "i", "in", "is", "it", "me", "my", "of", "on", "or", "our", "that", "the", "this", "to", "was",
    "what", "when", "where", "which", "who", "why", "with", "you", "your", "af", "at", "de", "den",
    "der", "det", "du", "en", "er", "et", "for", "har", "hvad", "hvor", "i", "jeg", "med", "og",
    "om", "på", "som", "til", "vi",
];

/// A search query in which any word may match (BM25 ranks passages with more and rarer
/// words higher). Words are quoted, so FTS syntax is literal; words of four or more letters
/// are prefix-matched ("refund" finds "refunds"). Stop words are dropped.
pub fn any_word_query(input: &str) -> Option<String> {
    let words: Vec<&str> = input
        .split(|c: char| !c.is_alphanumeric() && c != '\'' && c != '-')
        .map(|w| w.trim_matches(|c: char| c == '\'' || c == '-'))
        .filter(|w| w.chars().any(char::is_alphanumeric))
        .collect();
    let content: Vec<&str> = words
        .iter()
        .copied()
        .filter(|w| !STOP_WORDS.contains(&w.to_lowercase().as_str()))
        .collect();
    let chosen = if content.is_empty() { words } else { content };
    let terms: Vec<String> = chosen
        .into_iter()
        .take(32)
        .map(|w| {
            let quoted = format!("\"{}\"", w.replace('"', "\"\""));
            if w.chars().count() >= 4 {
                quoted + "*"
            } else {
                quoted
            }
        })
        .collect();
    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" OR "))
    }
}

impl Store {
    /// Adds a file's text to a project and indexes it.
    pub async fn add_knowledge_file(
        &self,
        project_id: &str,
        file: NewKnowledgeFile,
    ) -> Result<KnowledgeFile> {
        let text = file.text.trim();
        if text.is_empty() {
            return Err(StoreError::Invalid(format!(
                "“{}” has no text to add",
                file.name
            )));
        }
        let chars = text.chars().count();
        if chars > MAX_FILE_CHARS {
            return Err(StoreError::Invalid(format!(
                "“{}” is too long for project knowledge ({chars} characters; the limit is \
                 {MAX_FILE_CHARS})",
                file.name
            )));
        }
        let name: String = file.name.trim().chars().take(200).collect();
        let name = if name.is_empty() {
            "Untitled".to_owned()
        } else {
            name
        };
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM project_files WHERE project_id = ?")
                .bind(project_id)
                .fetch_one(&self.pool)
                .await?;
        if count >= MAX_PROJECT_FILES {
            return Err(StoreError::Invalid(format!(
                "a project holds at most {MAX_PROJECT_FILES} files"
            )));
        }
        let now = now_ms();
        let record = KnowledgeFile {
            id: new_id(),
            project_id: project_id.to_owned(),
            name,
            kind: file.kind,
            media_type: file.media_type,
            chars: u32::try_from(chars).unwrap_or(u32::MAX),
            tokens: estimate_tokens(chars),
            pages: file.pages,
            note: file.note,
            created_at: now,
        };
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO project_files
             (id, project_id, name, kind, media_type, chars, pages, note, text, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&record.id)
        .bind(project_id)
        .bind(&record.name)
        .bind(record.kind.as_str())
        .bind(&record.media_type)
        .bind(i64::from(record.chars))
        .bind(record.pages.map(i64::from))
        .bind(&record.note)
        .bind(text)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(|e| map_fk(e, "project"))?;
        for (position, (start, chunk)) in chunk_text(text).into_iter().enumerate() {
            sqlx::query(
                "INSERT INTO knowledge_chunks (file_id, project_id, position, start, text)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(&record.id)
            .bind(project_id)
            .bind(i64::try_from(position).unwrap_or(i64::MAX))
            .bind(i64::try_from(start).unwrap_or(i64::MAX))
            .bind(chunk)
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query("UPDATE projects SET updated_at = ? WHERE id = ?")
            .bind(now)
            .bind(project_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(record)
    }

    /// A project's files (without their text), oldest first.
    pub async fn knowledge_files(&self, project_id: &str) -> Result<Vec<KnowledgeFile>> {
        let rows: Vec<FileRow> = sqlx::query_as(&format!(
            "SELECT {FILE_COLUMNS} FROM project_files WHERE project_id = ?
             ORDER BY created_at, id"
        ))
        .bind(project_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    /// A project's files with their text, oldest first (the prompt when it fits).
    pub async fn knowledge_texts(&self, project_id: &str) -> Result<Vec<KnowledgeText>> {
        let files = self.knowledge_files(project_id).await?;
        let texts: Vec<(String, String)> = sqlx::query_as(
            "SELECT id, text FROM project_files WHERE project_id = ? ORDER BY created_at, id",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(files
            .into_iter()
            .zip(texts)
            .map(|(file, (_, text))| KnowledgeText { file, text })
            .collect())
    }

    /// One file with its text.
    pub async fn knowledge_file(&self, id: &str) -> Result<KnowledgeText> {
        let row: Option<FileRow> = sqlx::query_as(&format!(
            "SELECT {FILE_COLUMNS} FROM project_files WHERE id = ?"
        ))
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        let file: KnowledgeFile = row.ok_or(NOT_FOUND)?.into();
        let text: String = sqlx::query_scalar("SELECT text FROM project_files WHERE id = ?")
            .bind(id)
            .fetch_one(&self.pool)
            .await?;
        Ok(KnowledgeText { file, text })
    }

    pub async fn delete_knowledge_file(&self, id: &str) -> Result<()> {
        let done = sqlx::query("DELETE FROM project_files WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        if done.rows_affected() == 0 {
            return Err(NOT_FOUND);
        }
        Ok(())
    }

    /// The best passages of a project's knowledge for `query` (BM25), best first.
    pub async fn search_knowledge(
        &self,
        project_id: &str,
        query: &str,
        limit: u32,
    ) -> Result<Vec<KnowledgeHit>> {
        let Some(q) = any_word_query(query) else {
            return Ok(Vec::new());
        };
        let rows: Vec<(String, String, i64, i64, String, f64)> = sqlx::query_as(
            "SELECT c.file_id, f.name, c.position, c.start, c.text, bm25(knowledge_fts)
             FROM knowledge_fts
             JOIN knowledge_chunks c ON c.seq = knowledge_fts.rowid
             JOIN project_files f ON f.id = c.file_id
             WHERE knowledge_fts MATCH ? AND c.project_id = ?
             ORDER BY bm25(knowledge_fts)
             LIMIT ?",
        )
        .bind(&q)
        .bind(project_id)
        .bind(i64::from(limit.clamp(1, 50)))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(
                |(file_id, file_name, position, start, text, rank)| KnowledgeHit {
                    file_id,
                    file_name,
                    chunk: u32::try_from(position).unwrap_or(0),
                    start: u32::try_from(start).unwrap_or(0),
                    text,
                    // bm25() is lower for better matches.
                    score: -rank,
                },
            )
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::store;
    use oxide_core::NewProject;

    async fn project(s: &Store) -> String {
        s.create_project(NewProject {
            name: "P".into(),
            description: String::new(),
        })
        .await
        .expect("creates")
        .id
    }

    fn file(name: &str, text: &str) -> NewKnowledgeFile {
        NewKnowledgeFile {
            name: name.into(),
            kind: KnowledgeKind::Text,
            media_type: "text/plain".into(),
            pages: None,
            note: None,
            text: text.into(),
        }
    }

    #[test]
    fn chunks_overlap_and_break_at_paragraphs() {
        assert!(chunk_text("   ").is_empty());
        assert_eq!(chunk_text("short"), vec![(0, "short".to_owned())]);
        let para = "word ".repeat(200); // 1000 chars
        let text = format!("{para}\n\n{para}\n\n{para}");
        let chunks = chunk_text(&text);
        assert!(chunks.len() >= 2, "{}", chunks.len());
        for (_, c) in &chunks {
            assert!(c.chars().count() <= CHUNK_CHARS);
        }
        // The first passage ends at the blank line after the first paragraph.
        assert!(chunks[0].1.ends_with("word"));
        assert_eq!(chunks[0].1.chars().count(), 999);
        // Every character is in some passage.
        let (last_start, last) = chunks.last().expect("some");
        assert_eq!(
            last_start + last.chars().count(),
            text.trim_end().chars().count()
        );
        // Non-ASCII text without spaces still splits.
        let dense = "æ".repeat(4000);
        let chunks = chunk_text(&dense);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[1].0, CHUNK_CHARS - CHUNK_OVERLAP);
    }

    #[test]
    fn queries_match_any_word() {
        assert_eq!(any_word_query("  ?! "), None);
        assert_eq!(
            any_word_query("What is the refund policy\"; DROP it"),
            Some("\"refund\"* OR \"policy\"* OR \"DROP\"*".into())
        );
        // Only stop words: search for them anyway.
        assert_eq!(
            any_word_query("what is"),
            Some("\"what\"* OR \"is\"".into())
        );
    }

    #[tokio::test]
    async fn add_list_search_and_delete() {
        let s = store().await;
        let p = project(&s).await;
        let other = project(&s).await;
        let a = s
            .add_knowledge_file(
                &p,
                file(
                    "handbook.md",
                    "Our refund policy: customers get their money back within 30 days.",
                ),
            )
            .await
            .expect("adds");
        assert_eq!(a.tokens, estimate_tokens(a.chars as usize));
        s.add_knowledge_file(&p, file("notes.txt", "The office cat is named Ferris."))
            .await
            .expect("adds");
        s.add_knowledge_file(&other, file("x.txt", "refund refund refund"))
            .await
            .expect("adds");
        assert!(matches!(
            s.add_knowledge_file(&p, file("empty.txt", "  \n ")).await,
            Err(StoreError::Invalid(_))
        ));
        assert!(matches!(
            s.add_knowledge_file("missing", file("a", "b")).await,
            Err(StoreError::NotFound { .. })
        ));

        let files = s.knowledge_files(&p).await.expect("lists");
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].name, "handbook.md");
        let texts = s.knowledge_texts(&p).await.expect("texts");
        assert!(texts[1].text.contains("Ferris"));

        let hits = s
            .search_knowledge(&p, "What is the refund policy?", 5)
            .await
            .expect("searches");
        assert_eq!(hits.len(), 1, "only this project's files: {hits:?}");
        assert_eq!(hits[0].file_name, "handbook.md");
        assert!(hits[0].score > 0.0);
        // Prefix match and diacritics.
        let hits = s.search_knowledge(&p, "fërr", 5).await.expect("searches");
        assert_eq!(hits.len(), 1);
        assert!(
            s.search_knowledge(&p, "\"", 5)
                .await
                .expect("ok")
                .is_empty()
        );

        s.delete_knowledge_file(&a.id).await.expect("deletes");
        assert!(
            s.search_knowledge(&p, "refund", 5)
                .await
                .expect("ok")
                .is_empty()
        );
        assert!(matches!(
            s.knowledge_file(&a.id).await,
            Err(StoreError::NotFound { .. })
        ));
        // Deleting the project removes its files and their index.
        s.delete_project(&other).await.expect("deletes");
        assert!(
            s.search_knowledge(&other, "refund", 5)
                .await
                .expect("ok")
                .is_empty()
        );
    }

    #[tokio::test]
    async fn long_files_are_searchable_by_passage() {
        let s = store().await;
        let p = project(&s).await;
        let mut text = String::new();
        for i in 0..200 {
            text.push_str(&format!("Paragraph {i} talks about ordinary things.\n\n"));
        }
        text.push_str("The secret launch code is tangerine.\n\n");
        for i in 0..200 {
            text.push_str(&format!("Paragraph {i} again.\n\n"));
        }
        s.add_knowledge_file(&p, file("big.md", &text))
            .await
            .expect("adds");
        let hits = s
            .search_knowledge(&p, "launch code", 3)
            .await
            .expect("searches");
        assert!(hits[0].text.contains("tangerine"));
        assert!(hits[0].chunk > 0);
        let start = hits[0].start as usize;
        let from: String = text.chars().skip(start).take(20).collect();
        assert!(hits[0].text.starts_with(from.trim()));
    }
}
