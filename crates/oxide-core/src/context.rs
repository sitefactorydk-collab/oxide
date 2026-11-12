//! Context management: how much of a model's context window a chat uses, and the stored
//! summaries ("compactions") that stand in for the older part of a long chat.
//!
//! The transcript always keeps every message. A compaction only changes what later requests
//! send: the part of the thread it covers is replaced by its summary.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::Timestamp;

/// Context window assumed for a model whose window nobody reported (and no override names).
pub const DEFAULT_CONTEXT_WINDOW: u32 = 32_768;
/// Smallest and largest window an override may set.
pub const MIN_CONTEXT_WINDOW: u32 = 2_048;
pub const MAX_CONTEXT_WINDOW: u32 = 10_000_000;
/// Default share of the window (percent) at which old tool results are cleared, then the
/// conversation compacted.
pub const DEFAULT_COMPACT_THRESHOLD: u32 = 75;
/// Smallest and largest threshold Settings accepts (percent).
pub const MIN_COMPACT_THRESHOLD: u32 = 30;
pub const MAX_COMPACT_THRESHOLD: u32 = 95;

/// A user-set context window for one model (Settings → Models).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ContextWindowOverride {
    /// `<providerId>/<model>`.
    pub model: String,
    /// The window in tokens.
    pub tokens: u32,
}

/// Context management (Settings → Models).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct ContextSettings {
    /// Clear old tool results and compact long chats automatically.
    pub compaction: bool,
    /// Share of the context window (percent) at which that starts.
    pub threshold: u32,
    /// Model that writes summaries (`<providerId>/<model>`). `None`: the chat's own model.
    pub compaction_model: Option<String>,
    /// On Anthropic, let the API clear old tool results itself (context editing).
    pub anthropic_clearing: bool,
    /// Context windows set by hand, for models whose server doesn't report one.
    pub windows: Vec<ContextWindowOverride>,
}

impl Default for ContextSettings {
    fn default() -> Self {
        Self {
            compaction: true,
            threshold: DEFAULT_COMPACT_THRESHOLD,
            compaction_model: None,
            anthropic_clearing: true,
            windows: Vec::new(),
        }
    }
}

impl ContextSettings {
    /// Clamps the threshold and drops invalid or repeated overrides (the last one wins).
    pub fn normalized(mut self) -> Self {
        self.threshold = self
            .threshold
            .clamp(MIN_COMPACT_THRESHOLD, MAX_COMPACT_THRESHOLD);
        self.compaction_model = self
            .compaction_model
            .map(|m| m.trim().to_owned())
            .filter(|m| crate::model::split_model_id(m).is_some());
        let mut windows: Vec<ContextWindowOverride> = Vec::new();
        for w in self.windows.into_iter().rev() {
            let model = w.model.trim().to_owned();
            if crate::model::split_model_id(&model).is_none()
                || windows.iter().any(|o| o.model == model)
            {
                continue;
            }
            windows.push(ContextWindowOverride {
                model,
                tokens: w.tokens.clamp(MIN_CONTEXT_WINDOW, MAX_CONTEXT_WINDOW),
            });
        }
        windows.reverse();
        windows.truncate(200);
        self.windows = windows;
        self
    }

    /// The override for a model, if the user set one.
    pub fn window_for(&self, model_id: &str) -> Option<u32> {
        self.windows
            .iter()
            .find(|w| w.model == model_id)
            .map(|w| w.tokens)
    }
}

/// Why a compaction was made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum CompactionTrigger {
    /// The chat grew past the threshold.
    Auto,
    /// The user pressed **Compact now**.
    Manual,
    /// The provider answered that the context was full; Oxide compacted and tried again.
    Overflow,
    /// Not a summary: old tool results were replaced by short notes in the requests from
    /// here on (`summary` lists them). Only shown in the transcript.
    Cleared,
}

impl CompactionTrigger {
    /// The value stored in the database (same as the serde name).
    pub fn as_str(self) -> &'static str {
        match self {
            CompactionTrigger::Auto => "auto",
            CompactionTrigger::Manual => "manual",
            CompactionTrigger::Overflow => "overflow",
            CompactionTrigger::Cleared => "cleared",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "manual" => CompactionTrigger::Manual,
            "overflow" => CompactionTrigger::Overflow,
            "cleared" => CompactionTrigger::Cleared,
            _ => CompactionTrigger::Auto,
        }
    }
}

/// A summary that stands in for the start of a chat in later requests.
///
/// It covers the thread from the first message up to and including `message_id`. With
/// `after_tool_use_id` set, it covers that (assistant) message only up to the tool results
/// of the step that ended with that call: the reply was compacted while it was being
/// written, and its later steps are sent as they are.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Compaction {
    pub id: String,
    pub conversation_id: String,
    pub message_id: String,
    pub after_tool_use_id: Option<String>,
    /// The summary, in Markdown.
    pub summary: String,
    pub trigger: CompactionTrigger,
    /// Estimated prompt tokens before and after.
    pub tokens_before: u32,
    pub tokens_after: u32,
    /// The model that wrote the summary.
    pub model: Option<String>,
    pub created_at: Timestamp,
}

/// How full a chat's context is: what the next request would send, against the model's
/// window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ContextUsage {
    pub conversation_id: String,
    /// Estimated prompt tokens (system prompt, tools and messages).
    pub tokens: u32,
    /// The model's context window.
    pub window: u32,
    /// Where the window came from.
    pub window_source: WindowSource,
    /// Tokens at which old tool results are cleared and the chat compacted (0: off).
    pub threshold: u32,
    /// A stored summary stands in for the start of the chat.
    pub compacted: bool,
    /// Tool results replaced by short notes in the request (while a reply runs).
    pub cleared_results: u32,
}

/// Where a context window comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum WindowSource {
    /// The provider's model listing.
    Provider,
    /// Settings → Models.
    Setting,
    /// A "context full" answer named it.
    Learned,
    /// Unknown: Oxide's default.
    Default,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_normalizing() {
        let s: ContextSettings = serde_json::from_str("{}").expect("parses");
        assert!(s.compaction);
        assert!(s.anthropic_clearing);
        assert_eq!(s.threshold, DEFAULT_COMPACT_THRESHOLD);
        let s = ContextSettings {
            threshold: 5,
            compaction_model: Some(" nope ".into()),
            windows: vec![
                ContextWindowOverride {
                    model: "strata/swift".into(),
                    tokens: 10,
                },
                ContextWindowOverride {
                    model: "bad".into(),
                    tokens: 4096,
                },
                ContextWindowOverride {
                    model: " strata/swift ".into(),
                    tokens: 131_072,
                },
            ],
            ..ContextSettings::default()
        }
        .normalized();
        assert_eq!(s.threshold, MIN_COMPACT_THRESHOLD);
        assert_eq!(s.compaction_model, None);
        assert_eq!(s.windows.len(), 1);
        assert_eq!(s.window_for("strata/swift"), Some(131_072));
        assert_eq!(s.window_for("other/x"), None);
    }

    #[test]
    fn triggers_round_trip() {
        for t in [
            CompactionTrigger::Auto,
            CompactionTrigger::Manual,
            CompactionTrigger::Overflow,
            CompactionTrigger::Cleared,
        ] {
            assert_eq!(CompactionTrigger::parse(t.as_str()), t);
            let json = serde_json::to_string(&t).expect("serializes");
            assert_eq!(json, format!("\"{}\"", t.as_str()));
        }
    }
}
