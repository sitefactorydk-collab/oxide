//! Turning the visible thread into a model request.

use oxide_core::{
    ContentBlock, Message, ModelCapabilities, ProviderKind, Role, ThinkingStyle,
    settings::MIN_THINKING_BUDGET,
};
use oxide_llm::{ChatMessage, ChatRequest, Thinking};

/// Output tokens asked for when the model doesn't say how many it allows.
const FALLBACK_MAX_OUTPUT: u32 = 8_192;
/// Upper bound on output tokens for a normal reply.
const REPLY_MAX_OUTPUT: u32 = 32_000;
/// Room left for the answer after a thinking budget.
const ANSWER_ROOM: u32 = 4_096;

/// The conversation as the model sees it.
///
/// - Earlier thinking is left out (providers ignore or reject it in past turns).
/// - Tool calls and results are left out until the tool loop exists (Phase 6): a call without
///   its result would be rejected.
/// - Empty turns are skipped and consecutive turns of the same role merged, so roles
///   alternate starting with the user.
pub fn build_messages(thread: &[Message]) -> Vec<ChatMessage> {
    let mut out: Vec<ChatMessage> = Vec::new();
    for m in thread {
        let content: Vec<ContentBlock> = m
            .content
            .iter()
            .filter(|b| match b {
                ContentBlock::Text { text } => !text.trim().is_empty(),
                ContentBlock::Image { .. }
                | ContentBlock::Document { .. }
                | ContentBlock::ArtifactRef { .. } => true,
                ContentBlock::Thinking { .. }
                | ContentBlock::ToolUse { .. }
                | ContentBlock::ToolResult { .. } => false,
            })
            .cloned()
            .collect();
        if content.is_empty() {
            continue;
        }
        match out.last_mut() {
            Some(prev) if prev.role == m.role => prev.content.extend(content),
            _ => out.push(ChatMessage {
                role: m.role,
                content,
            }),
        }
    }
    while out.first().is_some_and(|m| m.role != Role::User) {
        out.remove(0);
    }
    out
}

/// The system prompt: who the assistant is, today's date, the user's name.
pub fn system_prompt(user_name: &str, now_ms: i64) -> String {
    let (y, m, d) = civil_date(now_ms);
    let mut s = format!(
        "You are Oxide, a helpful AI assistant in a desktop app. Today's date is {y:04}-{m:02}-{d:02} (UTC). \
         Format answers with Markdown where it helps; use fenced code blocks with a language for code."
    );
    let name = user_name.trim();
    if !name.is_empty() {
        s.push_str(&format!(" The user's name is {name}."));
    }
    s
}

/// Year, month and day (UTC) of a Unix time in milliseconds.
fn civil_date(ms: i64) -> (i64, u32, u32) {
    // Howard Hinnant's days-to-civil algorithm.
    let z = ms.div_euclid(86_400_000) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    (
        y,
        u32::try_from(m).unwrap_or(1),
        u32::try_from(d).unwrap_or(1),
    )
}

/// Fills in thinking, output limit and caching for a model.
///
/// `thinking_on` is the composer toggle; `budget` the configured thinking budget.
pub fn plan_request(
    request: &mut ChatRequest,
    kind: ProviderKind,
    caps: &ModelCapabilities,
    max_output: Option<u32>,
    thinking_on: bool,
    budget: u32,
) {
    if kind != ProviderKind::Anthropic {
        // OpenAI-compatible servers pick their own output limit and reasoning behavior.
        request.max_tokens = None;
        request.thinking = Thinking::Default;
        request.cache = false;
        return;
    }
    request.cache = true;
    let model_max = max_output
        .unwrap_or(FALLBACK_MAX_OUTPUT)
        .max(MIN_THINKING_BUDGET * 2);
    let normal = model_max.min(REPLY_MAX_OUTPUT);
    let (thinking, max) = match (caps.thinking, thinking_on) {
        (ThinkingStyle::Adaptive, true) => (Thinking::Adaptive, normal),
        // Only models that accept it are told not to think; the others use their default.
        (ThinkingStyle::Adaptive, false) if caps.thinking_off => (Thinking::Off, normal),
        (ThinkingStyle::Adaptive, false) => (Thinking::Default, normal),
        (ThinkingStyle::Budget, true) => {
            let max = model_max.min(normal.max(budget.saturating_add(ANSWER_ROOM)));
            let budget = budget.min(max.saturating_sub(ANSWER_ROOM.min(max / 2)));
            if budget < MIN_THINKING_BUDGET {
                (Thinking::Default, normal)
            } else {
                (Thinking::Budget(budget), max)
            }
        }
        _ => (Thinking::Default, normal),
    };
    request.thinking = thinking;
    request.max_tokens = Some(max);
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxide_core::{MediaSource, Message};

    fn msg(role: Role, content: Vec<ContentBlock>) -> Message {
        Message {
            id: "m".into(),
            conversation_id: "c".into(),
            parent_id: None,
            role,
            content,
            model: None,
            created_at: 0,
            stop_reason: None,
            usage: None,
        }
    }

    #[test]
    fn history_drops_thinking_and_tools_and_merges_turns() {
        let thread = vec![
            msg(Role::Assistant, vec![ContentBlock::text("orphan greeting")]),
            msg(Role::User, vec![ContentBlock::text("first")]),
            msg(
                Role::User,
                vec![ContentBlock::Image {
                    media_type: "image/png".into(),
                    source: MediaSource::Base64 { data: "x".into() },
                }],
            ),
            msg(
                Role::Assistant,
                vec![
                    ContentBlock::Thinking {
                        text: "hmm".into(),
                        signature: Some("s".into()),
                        duration_ms: None,
                    },
                    ContentBlock::text("answer"),
                    ContentBlock::ToolUse {
                        id: "t".into(),
                        name: "x".into(),
                        input_json: "{}".into(),
                    },
                ],
            ),
            msg(Role::Assistant, vec![ContentBlock::text("  ")]),
            msg(Role::User, vec![ContentBlock::text("second")]),
        ];
        let out = build_messages(&thread);
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].role, Role::User);
        assert_eq!(out[0].content.len(), 2);
        assert_eq!(out[1].content, vec![ContentBlock::text("answer")]);
        assert_eq!(out[2].content, vec![ContentBlock::text("second")]);
    }

    #[test]
    fn dates_are_utc_calendar_dates() {
        assert_eq!(civil_date(0), (1970, 1, 1));
        // 2026-11-05T12:00:00Z
        assert_eq!(civil_date(1_793_880_000_000), (2026, 11, 5));
        // 2024-02-29
        assert_eq!(civil_date(1_709_164_800_000), (2024, 2, 29));
        let p = system_prompt(" Ada ", 0);
        assert!(p.contains("1970-01-01") && p.ends_with("The user's name is Ada."));
        assert!(!system_prompt("", 0).contains("name"));
    }

    #[test]
    fn anthropic_thinking_and_limits() {
        let mut req = ChatRequest::new("m", vec![]);
        let budget = ModelCapabilities {
            thinking: ThinkingStyle::Budget,
            ..ModelCapabilities::default()
        };
        plan_request(
            &mut req,
            ProviderKind::Anthropic,
            &budget,
            Some(64_000),
            true,
            8_000,
        );
        assert_eq!(req.thinking, Thinking::Budget(8_000));
        assert_eq!(req.max_tokens, Some(32_000));
        assert!(req.cache);

        // A big budget raises the limit up to the model's maximum.
        plan_request(
            &mut req,
            ProviderKind::Anthropic,
            &budget,
            Some(64_000),
            true,
            60_000,
        );
        assert_eq!(req.max_tokens, Some(64_000));
        assert_eq!(req.thinking, Thinking::Budget(59_904));

        // Small models: the budget shrinks to leave room for the answer.
        plan_request(
            &mut req,
            ProviderKind::Anthropic,
            &budget,
            Some(8_192),
            true,
            8_000,
        );
        assert_eq!(req.max_tokens, Some(8_192));
        assert_eq!(req.thinking, Thinking::Budget(4_096));

        plan_request(
            &mut req,
            ProviderKind::Anthropic,
            &budget,
            None,
            false,
            8_000,
        );
        assert_eq!(req.thinking, Thinking::Default);

        let always_thinks = ModelCapabilities {
            thinking: ThinkingStyle::Adaptive,
            ..ModelCapabilities::default()
        };
        plan_request(
            &mut req,
            ProviderKind::Anthropic,
            &always_thinks,
            Some(128_000),
            false,
            8_000,
        );
        assert_eq!(
            req.thinking,
            Thinking::Default,
            "no `disabled` for always-on thinking"
        );

        let adaptive = ModelCapabilities {
            thinking: ThinkingStyle::Adaptive,
            thinking_off: true,
            ..ModelCapabilities::default()
        };
        plan_request(
            &mut req,
            ProviderKind::Anthropic,
            &adaptive,
            Some(128_000),
            false,
            8_000,
        );
        assert_eq!(req.thinking, Thinking::Off);
        plan_request(
            &mut req,
            ProviderKind::Anthropic,
            &adaptive,
            Some(128_000),
            true,
            8_000,
        );
        assert_eq!(req.thinking, Thinking::Adaptive);
        assert_eq!(req.max_tokens, Some(32_000));
    }

    #[test]
    fn openai_compatible_requests_use_server_defaults() {
        let mut req = ChatRequest::new("m", vec![]);
        req.max_tokens = Some(5);
        plan_request(
            &mut req,
            ProviderKind::LmStudio,
            &ModelCapabilities::default(),
            Some(9),
            true,
            8_000,
        );
        assert_eq!(req.max_tokens, None);
        assert_eq!(req.thinking, Thinking::Default);
        assert!(!req.cache);
    }
}
