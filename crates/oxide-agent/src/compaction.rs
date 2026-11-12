//! Context management (Phase 13b): how big a request is, how much room the model has, and
//! the two ways Oxide makes room when a chat grows past its threshold:
//!
//! 1. **Clearing old tool results.** Results older than the last few steps are replaced in
//!    the request by a short note naming the call, which stays, so the model sees what it
//!    already did and can call the tool again.
//! 2. **Compacting.** The oldest part of the conversation is summarized by a model, and the
//!    summary stands in for it. Summaries are stored ([`Compaction`]), so a chat is
//!    compacted once, not on every turn. The transcript keeps everything.
//!
//! Token counts are estimates (characters / 4), corrected per chat by what the provider
//! reported for the previous request.

use oxide_core::context::{DEFAULT_CONTEXT_WINDOW, WindowSource};
use oxide_core::{Compaction, ContentBlock, MediaSource, Message, ProviderKind, Role};
use oxide_llm::{ChatMessage, ChatRequest};

use crate::context::build_messages;
use crate::tools::{MAX_RESULT_CHARS, ToolOutput};

/// Tool-result turns kept in full when old results are cleared.
pub const KEEP_RESULT_TURNS: usize = 2;
/// Steps of the reply being written that stay word for word when it's compacted.
pub const KEEP_STEPS: usize = 2;
/// Results shorter than this aren't worth clearing.
const MIN_CLEARED_CHARS: usize = 400;
/// Estimated tokens of an image.
const IMAGE_TOKENS: u32 = 1_600;
/// Each message's own overhead (role, separators).
const MESSAGE_TOKENS: u32 = 4;
/// A tool result's text as the summarizer sees it, at most.
const SUMMARY_RESULT_CHARS: usize = 4_000;
/// The opening of a summary turn (also how one is recognized).
pub const SUMMARY_HEAD: &str = "[Context note from Oxide] The earlier part of this conversation \
                                was summarized to fit the model's context window.";

/// Rough token count of some text (characters / 4).
pub fn text_tokens(s: &str) -> u32 {
    let chars = s.chars().count();
    u32::try_from(chars.div_ceil(4)).unwrap_or(u32::MAX)
}

/// Estimated tokens of one block.
pub fn block_tokens(b: &ContentBlock) -> u32 {
    match b {
        ContentBlock::Text { text } => text_tokens(text),
        ContentBlock::Image { .. } => IMAGE_TOKENS,
        ContentBlock::Document {
            media_type, source, ..
        } => document_tokens(media_type, source),
        ContentBlock::Thinking { text, .. } => text_tokens(text),
        ContentBlock::ToolUse {
            name, input_json, ..
        } => text_tokens(name) + text_tokens(input_json) + 8,
        ContentBlock::ToolResult { content, .. } => content
            .iter()
            .map(block_tokens)
            .fold(8, u32::saturating_add),
        ContentBlock::ServerToolUse {
            name, input_json, ..
        } => text_tokens(name) + text_tokens(input_json) + 8,
        ContentBlock::ServerToolResult { raw_json, .. } => text_tokens(raw_json),
        ContentBlock::ArtifactRef { title, .. } => text_tokens(title) + 20,
    }
}

/// A document's tokens: text files by size, PDFs by a rough share of their bytes.
fn document_tokens(media_type: &str, source: &MediaSource) -> u32 {
    let bytes: u64 = match source {
        MediaSource::Base64 { data } => (data.len() as u64) / 4 * 3,
        MediaSource::Path { path } => std::fs::metadata(path).map(|m| m.len()).unwrap_or(0),
    };
    let tokens = if media_type.starts_with("text/") || media_type.contains("json") {
        bytes / 4
    } else {
        bytes / 40
    };
    u32::try_from(tokens.clamp(500, 200_000)).unwrap_or(200_000)
}

/// Estimated tokens of a list of messages.
pub fn messages_tokens(messages: &[ChatMessage]) -> u32 {
    messages
        .iter()
        .flat_map(|m| m.content.iter().map(block_tokens).chain([MESSAGE_TOKENS]))
        .fold(0, u32::saturating_add)
}

/// Estimated tokens of everything in a request except its messages: system prompt,
/// knowledge and tool definitions.
pub fn overhead_tokens(req: &ChatRequest) -> u32 {
    let mut n = req.system.as_deref().map(text_tokens).unwrap_or(0);
    n = n.saturating_add(req.system_context.as_deref().map(text_tokens).unwrap_or(0));
    for t in &req.tools {
        n = n
            .saturating_add(text_tokens(&t.name))
            .saturating_add(text_tokens(&t.description))
            .saturating_add(text_tokens(&t.input_schema.to_string()))
            .saturating_add(10);
    }
    for t in &req.server_tools {
        n = n.saturating_add(text_tokens(&t.to_string()));
    }
    n
}

/// The window assumed when nothing reports one, by provider.
pub fn default_window(kind: ProviderKind) -> u32 {
    match kind {
        ProviderKind::Anthropic => 200_000,
        ProviderKind::OpenAi => 128_000,
        ProviderKind::LmStudio | ProviderKind::Strata => DEFAULT_CONTEXT_WINDOW,
    }
}

/// How much room a model has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    pub window: u32,
    pub source: WindowSource,
    /// Tokens kept free for the answer.
    pub reserve: u32,
    /// Above this many prompt tokens, Oxide clears and compacts.
    pub trigger: u32,
}

impl Budget {
    /// `max_output`: the reply's output limit when the request sets one. `threshold`: percent
    /// of the window.
    pub fn new(window: u32, source: WindowSource, max_output: Option<u32>, threshold: u32) -> Self {
        let window = window.max(1_024);
        let reserve = max_output
            .unwrap_or_else(|| (window / 8).min(8_192))
            .min(window / 4);
        let by_share = u32::try_from(u64::from(window) * u64::from(threshold.min(100)) / 100)
            .unwrap_or(window);
        Self {
            window,
            source,
            reserve,
            trigger: by_share.min(window - reserve),
        }
    }

    /// The most prompt tokens a request can have and still leave room for the answer.
    pub fn hard_limit(&self) -> u32 {
        self.window - self.reserve
    }

    /// Clearing old results stops once the prompt is below this: 15 % of the window under
    /// the trigger, so it clears only what it must and isn't needed again at once.
    pub fn clear_goal(&self) -> u32 {
        self.trigger
            .saturating_sub(self.window / 100 * 15)
            .max(self.window / 4)
    }

    /// Characters one tool result may have: about a tenth of the window.
    pub fn result_cap_chars(&self) -> usize {
        (self.window as usize * 4 / 10).clamp(2_000, MAX_RESULT_CHARS)
    }
}

/// Number of user turns that carry tool results.
pub fn result_turns(messages: &[ChatMessage]) -> usize {
    messages.iter().filter(|m| has_results(m)).count()
}

fn has_results(m: &ChatMessage) -> bool {
    m.role == Role::User
        && m.content
            .iter()
            .any(|b| matches!(b, ContentBlock::ToolResult { .. }))
}

/// Replaces the results in the first `turns` result-carrying turns with short notes.
/// Returns how many results were cleared.
pub fn clear_results(messages: &mut [ChatMessage], turns: usize) -> u32 {
    if turns == 0 {
        return 0;
    }
    let calls: std::collections::HashMap<String, (String, String)> = messages
        .iter()
        .flat_map(|m| m.content.iter())
        .filter_map(|b| match b {
            ContentBlock::ToolUse {
                id,
                name,
                input_json,
                ..
            } => Some((id.clone(), (name.clone(), input_json.clone()))),
            _ => None,
        })
        .collect();
    let mut cleared = 0;
    let mut seen = 0;
    for m in messages.iter_mut() {
        if !has_results(m) {
            continue;
        }
        seen += 1;
        if seen > turns {
            break;
        }
        for b in &mut m.content {
            if let ContentBlock::ToolResult {
                tool_use_id,
                content,
                is_error,
            } = b
            {
                let chars: usize = content
                    .iter()
                    .map(|c| match c {
                        ContentBlock::Text { text } => text.chars().count(),
                        _ => MIN_CLEARED_CHARS,
                    })
                    .sum();
                if chars < MIN_CLEARED_CHARS || content.iter().any(is_stub) {
                    continue;
                }
                let (name, input) = calls
                    .get(tool_use_id.as_str())
                    .cloned()
                    .unwrap_or_else(|| ("the tool".into(), String::new()));
                *content = vec![ContentBlock::text(stub(&name, &input, chars, *is_error))];
                cleared += 1;
            }
        }
    }
    cleared
}

/// The calls whose results are cleared notes in `messages`: (call id, "tool detail").
pub fn cleared_calls(messages: &[ChatMessage]) -> Vec<(String, String)> {
    let calls: std::collections::HashMap<&str, (&str, &str)> = messages
        .iter()
        .flat_map(|m| m.content.iter())
        .filter_map(|b| match b {
            ContentBlock::ToolUse {
                id,
                name,
                input_json,
                ..
            } => Some((id.as_str(), (name.as_str(), input_json.as_str()))),
            _ => None,
        })
        .collect();
    messages
        .iter()
        .flat_map(|m| m.content.iter())
        .filter_map(|b| match b {
            ContentBlock::ToolResult {
                tool_use_id,
                content,
                ..
            } if content.iter().any(is_stub) => {
                let (name, input) = calls
                    .get(tool_use_id.as_str())
                    .copied()
                    .unwrap_or(("a tool", ""));
                let label = call_detail(input)
                    .map(|d| format!("{name} {d}"))
                    .unwrap_or_else(|| name.to_owned());
                Some((tool_use_id.clone(), label))
            }
            _ => None,
        })
        .collect()
}

/// How many results in `messages` are cleared notes.
pub fn cleared_count(messages: &[ChatMessage]) -> u32 {
    let n = messages
        .iter()
        .flat_map(|m| m.content.iter())
        .filter(|b| matches!(b, ContentBlock::ToolResult { content, .. } if content.iter().any(is_stub)))
        .count();
    u32::try_from(n).unwrap_or(u32::MAX)
}

fn is_stub(b: &ContentBlock) -> bool {
    matches!(b, ContentBlock::Text { text } if text.starts_with("[Result cleared"))
}

/// The note that replaces a cleared result.
fn stub(name: &str, input_json: &str, chars: usize, is_error: bool) -> String {
    let what = call_detail(input_json)
        .map(|d| format!("{name} {d}"))
        .unwrap_or_else(|| name.to_owned());
    let size = if chars >= 10_000 {
        format!("{}k characters", chars / 1000)
    } else {
        format!("{chars} characters")
    };
    let kind = if is_error { "error" } else { "result" };
    format!(
        "[Result cleared to save context: {what} · {kind} of {size}. Anything important from it \
         is in your earlier messages; call the tool again if you need it.]"
    )
}

/// The URL, path, query or command a call was about, shortened.
fn call_detail(input_json: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(input_json).ok()?;
    let o = v.as_object()?;
    let main = [
        "url", "path", "query", "command", "goal", "name", "ref", "text",
    ]
    .iter()
    .find_map(|k| o.get(*k).and_then(|x| x.as_str()))?;
    let mut s: String = main.chars().take(120).collect();
    if main.chars().count() > 120 {
        s.push('…');
    }
    if let Some(start) = o.get("start").and_then(|x| x.as_u64()) {
        s.push_str(&format!(" (from character {start})"));
    }
    Some(s)
}

/// Cuts a tool's output to `cap` characters of text, saying how much was left out.
pub fn fit_output(out: &mut ToolOutput, cap: usize) {
    let total: usize = out
        .content
        .iter()
        .map(|b| match b {
            ContentBlock::Text { text } => text.chars().count(),
            _ => 0,
        })
        .sum();
    if total <= cap {
        return;
    }
    let mut left = cap;
    for b in &mut out.content {
        if let ContentBlock::Text { text } = b {
            let n = text.chars().count();
            if n > left {
                let kept: String = text.chars().take(left).collect();
                *text = format!(
                    "{kept}\n\n[Cut to fit this model's context window: {} more characters \
                     weren't shown. Ask for a smaller part (for example with start / max_chars) \
                     if you need them.]",
                    n - left
                );
                left = 0;
            } else {
                left -= n;
            }
        }
    }
}

/// Length of the tail checked for repetition.
const REPEAT_WINDOW: usize = 6_000;
/// Share of distinct 32-character pieces below which text counts as repeating. Model prose
/// sits near 0.95, tables of fetched pages near 0.3, a loop like “No, 2017 Worlds MS
/// champion: No, 2017 …” near 0.01.
const REPEAT_RATIO: f64 = 0.1;

/// Share of distinct 32-character pieces in `s` (1.0: nothing repeats).
fn distinct_share(s: &[char]) -> f64 {
    const K: usize = 32;
    if s.len() <= K {
        return 1.0;
    }
    let n = s.len() - K;
    let set: std::collections::HashSet<&[char]> = (0..n).map(|i| &s[i..i + K]).collect();
    set.len() as f64 / n as f64
}

/// Whether the end of `text` is the same words over and over.
pub fn is_repeating(text: &str) -> bool {
    let tail: Vec<char> = text.chars().rev().take(REPEAT_WINDOW).collect();
    tail.len() == REPEAT_WINDOW && distinct_share(&tail) < REPEAT_RATIO
}

/// Where the repeating at the end of `text` started (a byte index on a character boundary),
/// to the nearest 1,000 characters.
pub fn repetition_start(text: &str) -> usize {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let only: Vec<char> = chars.iter().map(|(_, c)| *c).collect();
    let mut start = only.len().saturating_sub(REPEAT_WINDOW);
    while start >= 1_000 {
        let probe = &only[start - 1_000..(start - 1_000 + 2_000).min(only.len())];
        if distinct_share(probe) >= 0.3 {
            break;
        }
        start -= 1_000;
    }
    // Keep a little of the start of the loop, so it's clear what it was.
    let keep = (start + 300).min(only.len());
    chars.get(keep).map(|(i, _)| *i).unwrap_or(text.len())
}

/// The index just after the tool results of the step that ended with call `anchor`, in a
/// reply's blocks.
pub fn cut_after(content: &[ContentBlock], anchor: &str) -> Option<usize> {
    let at = content.iter().position(
        |b| matches!(b, ContentBlock::ToolResult { tool_use_id, .. } if tool_use_id == anchor),
    )?;
    let mut end = at + 1;
    while matches!(content.get(end), Some(ContentBlock::ToolResult { .. })) {
        end += 1;
    }
    Some(end)
}

/// The id of the last tool result before `cut` (what a compaction anchors to).
pub fn anchor_before(content: &[ContentBlock], cut: usize) -> Option<String> {
    match content.get(cut.checked_sub(1)?)? {
        ContentBlock::ToolResult { tool_use_id, .. } => Some(tool_use_id.clone()),
        _ => None,
    }
}

/// The text a summary turn carries.
pub fn summary_text(summary: &str) -> String {
    format!(
        "{SUMMARY_HEAD} Summary of everything before this point:\n\n{summary}\n\n[End of the \
         summary. What follows is the conversation as it continued.]"
    )
}

/// The latest compaction that applies to `thread` (its message is on the thread).
pub fn pick_compaction<'a>(thread: &[Message], all: &'a [Compaction]) -> Option<&'a Compaction> {
    all.iter().rev().find(|c| {
        c.trigger != oxide_core::CompactionTrigger::Cleared
            && thread.iter().any(|m| m.id == c.message_id)
    })
}

/// The thread as the model sees it when `c` stands in for its start. `None` when `c`'s
/// message isn't on the thread.
pub fn compacted_history(thread: &[Message], c: &Compaction) -> Option<Vec<ChatMessage>> {
    let at = thread.iter().position(|m| m.id == c.message_id)?;
    let first = &thread[at];
    let mut rest: Vec<Message> = Vec::with_capacity(thread.len() - at + 1);
    rest.push(Message {
        id: format!("{}-summary", c.id),
        role: Role::User,
        content: vec![ContentBlock::text(summary_text(&c.summary))],
        parent_id: None,
        model: None,
        stop_reason: None,
        usage: None,
        feedback: None,
        ..first.clone()
    });
    if let Some(anchor) = &c.after_tool_use_id
        && let Some(cut) = cut_after(&first.content, anchor)
    {
        rest.push(Message {
            content: first.content[cut..].to_vec(),
            ..first.clone()
        });
    }
    rest.extend(thread[at + 1..].iter().cloned());
    Some(build_messages(&rest))
}

/// The history a reply starts from: compacted when a stored summary applies.
pub fn history_for(thread: &[Message], all: &[Compaction]) -> (Vec<ChatMessage>, bool) {
    if let Some(c) = pick_compaction(thread, all)
        && let Some(h) = compacted_history(thread, c)
    {
        return (h, true);
    }
    (build_messages(thread), false)
}

pub const SUMMARY_SYSTEM: &str = "You compress conversations between a user and an AI \
assistant, so the assistant can continue the work with less context. You write a faithful, \
dense summary in Markdown. You never answer the user or continue the conversation yourself.";

/// The conversation as plain text for the summarizer. Long tool results are shortened.
pub fn transcript(messages: &[ChatMessage]) -> String {
    let mut s = String::new();
    for m in messages {
        for b in &m.content {
            match b {
                ContentBlock::Text { text } if !text.trim().is_empty() => {
                    let who = match m.role {
                        Role::User => "User",
                        Role::Assistant => "Assistant",
                    };
                    s.push_str(&format!("[{who}]\n{}\n\n", text.trim()));
                }
                ContentBlock::Image { .. } => s.push_str("[An image]\n\n"),
                ContentBlock::Document { name, .. } => {
                    s.push_str(&format!("[A document: {name}]\n\n"));
                }
                ContentBlock::ToolUse {
                    name, input_json, ..
                } => {
                    let input: String = input_json.chars().take(600).collect();
                    s.push_str(&format!("[Tool call: {name} {input}]\n\n"));
                }
                ContentBlock::ToolResult {
                    content, is_error, ..
                } => {
                    let text: String = content
                        .iter()
                        .filter_map(|c| match c {
                            ContentBlock::Text { text } => Some(text.as_str()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    let n = text.chars().count();
                    let mut shown: String = text.chars().take(SUMMARY_RESULT_CHARS).collect();
                    if n > SUMMARY_RESULT_CHARS {
                        shown
                            .push_str(&format!(" […{} more characters]", n - SUMMARY_RESULT_CHARS));
                    }
                    let label = if *is_error {
                        "Tool error"
                    } else {
                        "Tool result"
                    };
                    s.push_str(&format!("[{label}]\n{}\n\n", shown.trim()));
                }
                ContentBlock::ServerToolUse {
                    name, input_json, ..
                } => s.push_str(&format!("[Provider tool: {name} {input_json}]\n\n")),
                ContentBlock::ArtifactRef { title, version, .. } => {
                    s.push_str(&format!("[Artifact “{title}”, version {version}]\n\n"));
                }
                _ => {}
            }
        }
    }
    s
}

/// Splits a transcript into pieces of at most `max` characters, at a line end when one is
/// near the cut.
pub fn chunks(text: &str, max: usize) -> Vec<String> {
    let max = max.max(500);
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut at = 0;
    while at < chars.len() {
        let mut end = (at + max).min(chars.len());
        if end < chars.len() {
            let floor = end - max / 5;
            if let Some(nl) = (floor..end).rev().find(|&i| chars[i] == '\n') {
                end = nl + 1;
            }
        }
        out.push(chars[at..end].iter().collect());
        at = end;
    }
    out
}

/// The request text for one summarizing pass.
pub fn summary_prompt(
    so_far: Option<&str>,
    transcript: &str,
    part: Option<(usize, usize)>,
    words: u32,
) -> String {
    let mut p = String::from(
        "Below is the start of a conversation that no longer fits the model's context window. \
         Write a summary that will replace it. The assistant continues from your summary alone, \
         so keep everything it needs, under these headings:\n\n\
         ## Task\nWhat the user asked for (in their words where it matters), with constraints \
         and preferences.\n\
         ## Progress\nWhat was done so far: decisions, files and artifacts made (names and \
         paths), tools used and what they gave.\n\
         ## Facts found\nEvery fact, figure, name and date gathered that the task needs, each \
         with its source (URL, file or tool). Be complete and precise: this is the only copy.\n\
         ## Next steps\nWhat is left to do, and what failed and shouldn't be tried the same way \
         again.\n\n\
         If the transcript starts with an earlier summary, fold it in.",
    );
    if let Some((i, n)) = part {
        p.push_str(&format!(
            " The transcript comes in {n} parts; this is part {i}. Extend the summary so far \
             with what this part adds, and answer with the whole updated summary."
        ));
    }
    p.push_str(&format!(
        " Keep it under about {words} words. Answer with the summary only.\n\n"
    ));
    if let Some(s) = so_far {
        p.push_str(&format!("<summary_so_far>\n{s}\n</summary_so_far>\n\n"));
    }
    p.push_str(&format!("<transcript>\n{transcript}</transcript>"));
    p
}

/// Adds the user's latest request word for word to a summary made while answering it.
pub fn with_latest_request(summary: &str, request: &str) -> String {
    let request = request.trim();
    if request.is_empty() {
        return summary.trim().to_owned();
    }
    let mut r: String = request.chars().take(4_000).collect();
    if request.chars().count() > 4_000 {
        r.push('…');
    }
    let quoted = r
        .lines()
        .map(|l| format!("> {l}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "{}\n\n## The user's latest request (word for word)\n{quoted}",
        summary.trim()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turn(role: Role, content: Vec<ContentBlock>) -> ChatMessage {
        ChatMessage { role, content }
    }

    fn use_(id: &str, json: &str) -> ContentBlock {
        ContentBlock::ToolUse {
            id: id.into(),
            name: "web_fetch".into(),
            input_json: json.into(),
            server: None,
            tool: None,
        }
    }

    fn result(id: &str, text: &str) -> ContentBlock {
        ContentBlock::ToolResult {
            tool_use_id: id.into(),
            content: vec![ContentBlock::text(text)],
            is_error: false,
        }
    }

    #[test]
    fn budgets_leave_room_for_the_answer() {
        let b = Budget::new(131_072, WindowSource::Provider, None, 75);
        assert_eq!(b.reserve, 8_192);
        assert_eq!(b.trigger, 98_304);
        assert_eq!(b.hard_limit(), 122_880);
        assert_eq!(b.result_cap_chars(), 52_428);
        // A small window: the reserve takes an eighth, the trigger stays below the limit.
        let b = Budget::new(4_096, WindowSource::Default, None, 95);
        assert_eq!(b.reserve, 512);
        assert_eq!(b.trigger, 3_584);
        assert_eq!(b.result_cap_chars(), 2_000);
        // Anthropic's output limit is the reserve, at most a quarter of the window.
        let b = Budget::new(200_000, WindowSource::Provider, Some(64_000), 75);
        assert_eq!(b.reserve, 50_000);
        assert_eq!(b.trigger, 150_000);
    }

    #[test]
    fn old_results_are_cleared_with_a_note_and_the_calls_stay() {
        let big = "x".repeat(40_000);
        let mut m = vec![
            turn(Role::User, vec![ContentBlock::text("research this")]),
            turn(
                Role::Assistant,
                vec![use_(
                    "a",
                    r#"{"url":"https://en.wikipedia.org/wiki/A","start":100}"#,
                )],
            ),
            turn(Role::User, vec![result("a", &big)]),
            turn(Role::Assistant, vec![use_("b", r#"{"url":"https://b"}"#)]),
            turn(Role::User, vec![result("b", "short")]),
            turn(Role::Assistant, vec![use_("c", r#"{"url":"https://c"}"#)]),
            turn(Role::User, vec![result("c", &big)]),
        ];
        assert_eq!(result_turns(&m), 3);
        let before = messages_tokens(&m);
        assert_eq!(clear_results(&mut m, 2), 1, "short results stay");
        let ContentBlock::ToolResult { content, .. } = &m[2].content[0] else {
            panic!("a result");
        };
        let ContentBlock::Text { text } = &content[0] else {
            panic!("text");
        };
        assert!(text.starts_with("[Result cleared"), "{text}");
        assert!(text.contains("web_fetch https://en.wikipedia.org/wiki/A (from character 100)"));
        assert!(text.contains("40k characters"));
        assert!(matches!(&m[1].content[0], ContentBlock::ToolUse { .. }));
        assert!(messages_tokens(&m) < before - 9_000);
        // The latest results are kept, and clearing again changes nothing.
        assert_eq!(clear_results(&mut m, 2), 0);
        assert!(
            matches!(&m[6].content[0], ContentBlock::ToolResult { content, .. }
            if matches!(&content[0], ContentBlock::Text { text } if text.len() == 40_000))
        );
    }

    #[test]
    fn repetition_is_found_and_cut_where_it_starts() {
        let prose: String = (0..400)
            .map(|i| {
                format!(
                    "Sentence {i} talks about player number {} and a title. ",
                    i * 7
                )
            })
            .collect();
        assert!(!is_repeating(&prose));
        let table: String = (0..300)
            .map(|i| format!("| {} | Viktor Axelsen | Denmark | {} |\n", 2000 + i, i % 9))
            .collect();
        assert!(!is_repeating(&table));
        let looped = format!("{prose}{}", "No, 2017 Worlds MS champion: ".repeat(600));
        assert!(is_repeating(&looped));
        let cut = repetition_start(&looped);
        assert!(
            cut >= prose.len() - 1_000 && cut <= prose.len() + 1_300,
            "{cut} vs {}",
            prose.len()
        );
        assert!(looped.is_char_boundary(cut));
    }

    #[test]
    fn outputs_are_cut_to_the_cap() {
        let mut out = ToolOutput {
            content: vec![ContentBlock::text("a".repeat(5_000))],
            is_error: false,
            duration_ms: 0,
        };
        fit_output(&mut out, 2_000);
        let ContentBlock::Text { text } = &out.content[0] else {
            panic!("text");
        };
        assert!(text.starts_with(&"a".repeat(2_000)));
        assert!(text.contains("3000 more characters"));
    }

    #[test]
    fn anchors_find_the_end_of_a_step() {
        let content = vec![
            ContentBlock::text("Looking"),
            use_("a", "{}"),
            use_("b", "{}"),
            result("a", "1"),
            result("b", "2"),
            ContentBlock::text("Next"),
        ];
        assert_eq!(cut_after(&content, "a"), Some(5));
        assert_eq!(cut_after(&content, "b"), Some(5));
        assert_eq!(cut_after(&content, "zzz"), None);
        assert_eq!(anchor_before(&content, 5).as_deref(), Some("b"));
        assert_eq!(anchor_before(&content, 1), None);
        assert_eq!(anchor_before(&content, 0), None);
    }

    fn msg(id: &str, role: Role, content: Vec<ContentBlock>) -> Message {
        Message {
            id: id.into(),
            conversation_id: "c".into(),
            parent_id: None,
            role,
            content,
            model: None,
            created_at: 0,
            stop_reason: None,
            usage: None,
            feedback: None,
        }
    }

    fn compaction(message: &str, anchor: Option<&str>) -> Compaction {
        Compaction {
            id: "k".into(),
            conversation_id: "c".into(),
            message_id: message.into(),
            after_tool_use_id: anchor.map(str::to_owned),
            summary: "SUMMARY".into(),
            trigger: oxide_core::CompactionTrigger::Auto,
            tokens_before: 0,
            tokens_after: 0,
            model: None,
            created_at: 0,
        }
    }

    #[test]
    fn stored_summaries_stand_in_for_the_start_of_the_thread() {
        let thread = vec![
            msg("u1", Role::User, vec![ContentBlock::text("first")]),
            msg(
                "a1",
                Role::Assistant,
                vec![
                    use_("t1", "{}"),
                    result("t1", "old"),
                    use_("t2", "{}"),
                    result("t2", "newer"),
                    ContentBlock::text("Done."),
                ],
            ),
            msg("u2", Role::User, vec![ContentBlock::text("second")]),
        ];
        // Through a whole message: the summary, then the rest.
        let h = compacted_history(&thread, &compaction("a1", None)).expect("applies");
        assert_eq!(h.len(), 1, "summary and next user message merge");
        assert_eq!(h[0].content.len(), 2);
        assert!(
            matches!(&h[0].content[0], ContentBlock::Text { text } if text.contains("SUMMARY"))
        );
        // Inside a reply: its later steps follow the summary.
        let h = compacted_history(&thread, &compaction("a1", Some("t1"))).expect("applies");
        assert_eq!(h.len(), 5);
        assert_eq!(h[0].role, Role::User);
        assert_eq!(h[4].role, Role::User);
        assert!(matches!(&h[1].content[0], ContentBlock::ToolUse { id, .. } if id == "t2"));
        assert!(
            matches!(&h[2].content[0], ContentBlock::ToolResult { tool_use_id, .. } if tool_use_id == "t2")
        );
        assert_eq!(h[3].role, Role::Assistant);
        // A compaction on another branch doesn't apply.
        let all = vec![compaction("elsewhere", None)];
        let (h, compacted) = history_for(&thread, &all);
        assert!(!compacted);
        assert_eq!(h, build_messages(&thread));
        let all = vec![compaction("u1", None), compaction("a1", Some("t1"))];
        assert_eq!(
            pick_compaction(&thread, &all).map(|c| c.message_id.as_str()),
            Some("a1")
        );
    }

    #[test]
    fn transcripts_shorten_results_and_split_into_chunks() {
        let m = vec![
            turn(Role::User, vec![ContentBlock::text("find X")]),
            turn(Role::Assistant, vec![use_("a", r#"{"url":"u"}"#)]),
            turn(Role::User, vec![result("a", &"y".repeat(10_000))]),
        ];
        let t = transcript(&m);
        assert!(t.contains("[User]\nfind X"));
        assert!(t.contains("[Tool call: web_fetch {\"url\":\"u\"}]"));
        assert!(t.contains("[…6000 more characters]"));
        let parts = chunks(&t, 1_000);
        assert!(parts.len() >= 4);
        assert!(parts.iter().all(|p| p.chars().count() <= 1_000));
        assert_eq!(parts.concat(), t);
        let p = summary_prompt(Some("old"), "T", Some((2, 3)), 800);
        assert!(p.contains("part 2"));
        assert!(p.contains("<summary_so_far>\nold\n</summary_so_far>"));
        assert!(p.ends_with("<transcript>\nT</transcript>"));
        let s = with_latest_request("S", "make a sheet\nplease");
        assert!(s.ends_with("> make a sheet\n> please"));
    }
}
