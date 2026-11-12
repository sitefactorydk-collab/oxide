//! Errors from the model APIs. Messages are written to be shown to the user as they are and
//! never contain API keys or request headers.

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("couldn't reach {url}: {reason}")]
    Unreachable { url: String, reason: String },
    #[error("the API key was rejected ({status})")]
    Unauthorized { status: u16 },
    #[error("no API key is set for this endpoint. Add one in Settings → Providers.")]
    MissingKey,
    #[error("the provider is rate limiting requests: {message}")]
    RateLimited { message: String },
    #[error("the provider is overloaded right now. Try again in a moment.")]
    Overloaded,
    #[error("the server answered {status}: {message}")]
    Http { status: u16, message: String },
    #[error("unexpected response: {0}")]
    Decode(String),
    #[error("invalid request: {0}")]
    Invalid(String),
    /// The reply broke off part-way (a stream error event or a dropped connection).
    #[error("{0}")]
    Stream(String),
}

impl LlmError {
    /// Whether waiting and sending the same request again may succeed.
    pub fn is_retryable(&self) -> bool {
        matches!(self, LlmError::RateLimited { .. } | LlmError::Overloaded)
    }
    /// Whether the provider said the prompt doesn't fit the model's context window
    /// (Anthropic "prompt is too long", OpenAI `context_length_exceeded`, llama.cpp's
    /// `exceed_context_size_error`, LM Studio and Strata's own words).
    pub fn is_context_full(&self) -> bool {
        match self {
            LlmError::Http { status, message } => {
                matches!(status, 400 | 413 | 422 | 500 | 503) && mentions_context_full(message)
            }
            LlmError::Stream(message) | LlmError::Invalid(message) => {
                mentions_context_full(message)
            }
            _ => false,
        }
    }

    /// The prompt size and window a "context full" error names, when it does.
    pub fn context_numbers(&self) -> ContextNumbers {
        match self {
            LlmError::Http { message, .. } | LlmError::Stream(message) => context_numbers(message),
            _ => ContextNumbers::default(),
        }
    }
}

/// Numbers a "context full" error message gives.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ContextNumbers {
    /// Tokens the prompt had.
    pub prompt: Option<u32>,
    /// The model's context window.
    pub window: Option<u32>,
}

const CONTEXT_FULL_PHRASES: &[&str] = &[
    "context window",
    "context length",
    "context_length",
    "context size",
    "context_size",
    "maximum context",
    "prompt is too long",
    "prompt too long",
    "input is too long",
    "too many tokens",
    "exceeds the context",
    "exceed_context",
    "context overflow",
    "context the overflows",
    "n_ctx",
];

fn mentions_context_full(message: &str) -> bool {
    let m = message.to_lowercase();
    CONTEXT_FULL_PHRASES.iter().any(|p| m.contains(p))
}

/// Pulls the prompt size and the window out of a "context full" message. Every provider
/// words it differently, so this reads each number with the words just before and after it:
///
/// - Strata: "prompt (132633 tokens) leaves no room to answer in the context (131072)"
/// - OpenAI: "maximum context length is 128000 tokens. However, your messages resulted in
///   130000 tokens"
/// - llama.cpp: "request (8000 tokens) exceeds the available context size (4096 tokens)"
/// - LM Studio: "Trying to keep the first 5000 tokens … the model is loaded with context
///   length of 4096 tokens"
/// - Anthropic: "prompt is too long: 210000 tokens > 200000 maximum"
pub fn context_numbers(message: &str) -> ContextNumbers {
    let lower = message.to_lowercase();
    let chars: Vec<char> = lower.chars().collect();
    let mut out = ContextNumbers::default();
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let start = i;
        let mut digits = String::new();
        while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == ',') {
            if chars[i] != ',' {
                digits.push(chars[i]);
            }
            i += 1;
        }
        let Ok(n) = digits.parse::<u64>() else {
            continue;
        };
        let Ok(n) = u32::try_from(n) else {
            continue;
        };
        if n < 256 {
            continue;
        }
        let before: String = chars[start.saturating_sub(40)..start].iter().collect();
        let after: String = chars[i..(i + 16).min(chars.len())].iter().collect();
        let window_words = before.contains("context")
            || before.contains("n_ctx")
            || before.contains("maximum")
            || before.contains("> ")
            || after.trim_start().starts_with("maximum");
        let prompt_words = before.contains("prompt")
            || before.contains("request")
            || before.contains("resulted in")
            || before.contains("keep the first")
            || before.contains("too long:")
            || before.contains("n_prompt");
        // The nearest words decide: "prompt (132633 tokens) … context (131072)".
        let near: String = chars[start.saturating_sub(12)..start].iter().collect();
        if near.contains("context") || near.contains("n_ctx") || near.contains("> ") {
            out.window.get_or_insert(n);
        } else if near.contains("prompt")
            || near.contains("request")
            || near.contains("resulted in")
        {
            out.prompt.get_or_insert(n);
        } else if window_words && !prompt_words {
            out.window.get_or_insert(n);
        } else if prompt_words && !window_words {
            out.prompt.get_or_insert(n);
        } else if window_words {
            out.window.get_or_insert(n);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn http(message: &str) -> LlmError {
        LlmError::Http {
            status: 400,
            message: message.into(),
        }
    }

    #[test]
    fn context_full_answers_are_recognized_with_their_numbers() {
        let cases: [(&str, Option<u32>, Option<u32>); 5] = [
            (
                "request exceeds the context window: prompt (132633 tokens) leaves no room to \
                 answer in the context (131072); requests are never truncated",
                Some(132_633),
                Some(131_072),
            ),
            (
                "This model's maximum context length is 128000 tokens. However, your messages \
                 resulted in 130000 tokens. Please reduce the length of the messages.",
                Some(130_000),
                Some(128_000),
            ),
            (
                "request (8000 tokens) exceeds the available context size (4096 tokens), try \
                 increasing it",
                Some(8000),
                Some(4096),
            ),
            (
                "Trying to keep the first 5000 tokens when context the overflows. However, the \
                 model is loaded with context length of 4096 tokens",
                Some(5000),
                Some(4096),
            ),
            (
                "prompt is too long: 210000 tokens > 200000 maximum",
                Some(210_000),
                Some(200_000),
            ),
        ];
        for (m, prompt, window) in cases {
            let e = http(m);
            assert!(e.is_context_full(), "{m}");
            assert_eq!(
                e.context_numbers(),
                ContextNumbers { prompt, window },
                "{m}"
            );
        }
    }

    #[test]
    fn other_errors_are_not_context_full() {
        assert!(!http("invalid model name").is_context_full());
        assert!(!LlmError::Unauthorized { status: 401 }.is_context_full());
        assert!(
            !LlmError::Http {
                status: 429,
                message: "too many tokens per minute".into()
            }
            .is_context_full()
        );
        assert!(LlmError::Stream("the context size was exceeded".into()).is_context_full());
        assert_eq!(context_numbers("nothing here"), ContextNumbers::default());
    }
}
