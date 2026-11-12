//! Full-text search over chat titles and message text (SQLite FTS5).

use std::collections::HashMap;
use std::collections::hash_map::Entry;

use oxide_core::{Conversation, SearchHit};

use crate::conversations::{CONVERSATION_COLUMNS, ConversationRow};
use crate::{Result, Store};

/// Most message matches considered per search, before grouping by chat.
const MESSAGE_CANDIDATES: i64 = 400;

/// Turns free text into a safe FTS5 query: every word is quoted (so FTS syntax characters are
/// literal) and prefix-matched, and all words must match. `None` if nothing searchable is left.
pub fn fts_query(input: &str) -> Option<String> {
    let terms: Vec<String> = input
        .split_whitespace()
        .filter(|w| w.chars().any(char::is_alphanumeric))
        .map(|w| format!("\"{}\"*", w.replace('"', "\"\"")))
        .collect();
    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" "))
    }
}

impl Store {
    /// Chats whose title or message text match `query`, best first: title matches, then chats
    /// ranked by their best message match. Message hits carry a snippet with the terms
    /// wrapped in U+0002 / U+0003.
    pub async fn search(&self, query: &str, limit: u32) -> Result<Vec<SearchHit>> {
        let Some(q) = fts_query(query) else {
            return Ok(Vec::new());
        };
        let limit = usize::try_from(limit.clamp(1, 200)).unwrap_or(200);

        let title_ids: Vec<String> = sqlx::query_scalar(
            "SELECT c.id FROM conversations_fts f
             JOIN conversations c ON c.seq = f.rowid
             WHERE conversations_fts MATCH ?
             ORDER BY f.rank, c.updated_at DESC",
        )
        .bind(&q)
        .fetch_all(&self.pool)
        .await?;

        let message_hits: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT m.conversation_id, m.id,
                    snippet(messages_fts, 0, char(2), char(3), '…', 12)
             FROM messages_fts f
             JOIN messages m ON m.seq = f.rowid
             WHERE messages_fts MATCH ?
             ORDER BY f.rank
             LIMIT ?",
        )
        .bind(&q)
        .bind(MESSAGE_CANDIDATES)
        .fetch_all(&self.pool)
        .await?;

        // Order: title hits, then message-only hits by best rank. Keep each chat's best snippet.
        let mut order: Vec<String> = Vec::new();
        let mut snippets: HashMap<String, (String, String)> = HashMap::new();
        for id in title_ids {
            if !order.contains(&id) {
                order.push(id);
            }
        }
        for (conversation_id, message_id, snippet) in message_hits {
            if let Entry::Vacant(slot) = snippets.entry(conversation_id) {
                if !order.contains(slot.key()) {
                    order.push(slot.key().clone());
                }
                slot.insert((message_id, snippet));
            }
        }
        order.truncate(limit);

        let mut by_id = self.conversations_by_ids(&order).await?;
        Ok(order
            .into_iter()
            .filter_map(|id| {
                let conversation = by_id.remove(&id)?;
                let (message_id, snippet) = snippets.remove(&id).unzip();
                Some(SearchHit {
                    conversation,
                    message_id,
                    snippet,
                })
            })
            .collect())
    }

    async fn conversations_by_ids(&self, ids: &[String]) -> Result<HashMap<String, Conversation>> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let placeholders = vec!["?"; ids.len()].join(", ");
        let sql = format!(
            "SELECT {CONVERSATION_COLUMNS} FROM conversations WHERE id IN ({placeholders})"
        );
        let mut query = sqlx::query_as::<_, ConversationRow>(&sql);
        for id in ids {
            query = query.bind(id);
        }
        let rows = query.fetch_all(&self.pool).await?;
        Ok(rows
            .into_iter()
            .map(|r| {
                let c = Conversation::from(r);
                (c.id.clone(), c)
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::store;
    use oxide_core::{ContentBlock, Role, StartConversation};

    async fn chat(s: &Store, text: &str) -> Conversation {
        s.start_conversation(StartConversation {
            text: text.into(),
            model: "m".into(),
            project_id: None,
            attachments: Vec::new(),
            folders: vec![],
            tools: None,
        })
        .await
        .expect("creates")
        .0
    }

    #[test]
    fn query_quotes_terms_and_prefix_matches() {
        assert_eq!(
            fts_query("tauri  win").as_deref(),
            Some("\"tauri\"* \"win\"*")
        );
        assert_eq!(fts_query("a\"b OR").as_deref(), Some("\"a\"\"b\"* \"OR\"*"));
        assert_eq!(fts_query(" -- ** ( "), None);
    }

    #[tokio::test]
    async fn finds_message_text_with_snippet() {
        let s = store().await;
        let c = chat(&s, "Window chrome").await;
        let root = s.list_messages(&c.id).await.expect("ok").remove(0);
        s.add_message(
            &c.id,
            Some(&root.id),
            Role::Assistant,
            vec![ContentBlock::text(
                "Add the capability core:window:allow-start-dragging.",
            )],
            None,
        )
        .await
        .expect("adds");
        chat(&s, "Unrelated").await;

        let hits = s.search("capab", 10).await.expect("searches");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].conversation.id, c.id);
        let snippet = hits[0].snippet.as_deref().expect("has snippet");
        assert!(snippet.contains("\u{2}capability\u{3}"), "{snippet}");
    }

    #[tokio::test]
    async fn title_hits_come_first_and_follow_renames() {
        let s = store().await;
        let body = chat(&s, "zebra appears in the body only").await;
        s.rename_conversation(&body.id, "Animals")
            .await
            .expect("renames");
        let titled = chat(&s, "Something").await;
        s.rename_conversation(&titled.id, "Zebra notes")
            .await
            .expect("renames");

        let hits = s.search("zebra", 10).await.expect("searches");
        let ids: Vec<_> = hits.iter().map(|h| h.conversation.id.clone()).collect();
        assert_eq!(ids, vec![titled.id.clone(), body.id.clone()]);
        // A title hit has no snippet; a hit in the first message (old title text) has one.
        assert_eq!(hits[0].snippet, None);
        let by_text = s.search("something", 10).await.expect("ok");
        assert_eq!(by_text.len(), 1);
        assert!(by_text[0].snippet.is_some());
    }

    #[tokio::test]
    async fn ignores_diacritics_and_is_case_insensitive() {
        let s = store().await;
        let c = chat(&s, "Møde på Café Ørsted").await;
        assert_eq!(
            s.search("cafe", 5).await.expect("ok")[0].conversation.id,
            c.id
        );
        assert_eq!(s.search("MØDE", 5).await.expect("ok").len(), 1);
    }

    #[tokio::test]
    async fn deleted_chats_leave_the_index() {
        let s = store().await;
        let c = chat(&s, "ephemeral words").await;
        s.delete_conversation(&c.id).await.expect("deletes");
        assert!(s.search("ephemeral", 5).await.expect("ok").is_empty());
    }

    #[tokio::test]
    async fn fts_syntax_in_queries_is_harmless() {
        let s = store().await;
        chat(&s, "NEAR AND OR NOT").await;
        for q in ["NEAR(", "\"", "a*b", "title:x", "-x", "^"] {
            s.search(q, 5).await.expect("never a syntax error");
        }
    }
}
