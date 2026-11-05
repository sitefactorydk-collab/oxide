//! Automatic chat titles after the first exchange.

use oxide_core::{ModelInfo, ProviderKind};

/// Longest automatic title, in characters.
const MAX_TITLE_CHARS: usize = 60;

/// The prompt asking for a title.
pub(crate) fn title_prompt(user_text: &str, reply_text: &str) -> String {
    let user: String = user_text.chars().take(2_000).collect();
    let reply: String = reply_text.chars().take(1_000).collect();
    format!(
        "Write a short title (at most 6 words) for a chat that starts like this. \
         Reply with the title only: no quotes, no trailing period.\n\n\
         <user>\n{user}\n</user>\n<assistant>\n{reply}\n</assistant>"
    )
}

/// Cleans a model's answer into a title: first non-empty line, without quotes, Markdown
/// heading marks, a "Title:" prefix or a trailing period. `None` if nothing usable is left.
pub fn clean_title(raw: &str) -> Option<String> {
    let line = raw.lines().map(str::trim).find(|l| !l.is_empty())?;
    let mut t = line.trim_start_matches('#').trim();
    for prefix in ["Title:", "title:", "TITLE:"] {
        if let Some(rest) = t.strip_prefix(prefix) {
            t = rest.trim();
        }
    }
    let t = t
        .trim_matches(|c: char| matches!(c, '"' | '\'' | '`' | '*' | '“' | '”' | '‘' | '’'))
        .trim()
        .trim_end_matches('.')
        .trim();
    if t.is_empty() {
        return None;
    }
    let t: String = if t.chars().count() > MAX_TITLE_CHARS {
        let cut: String = t.chars().take(MAX_TITLE_CHARS - 1).collect();
        format!("{}…", cut.trim_end())
    } else {
        t.to_owned()
    };
    Some(t)
}

/// A cheap model from the chat's endpoint for titles, or `None` to use the chat model.
pub fn pick_title_model(kind: ProviderKind, models: &[ModelInfo]) -> Option<String> {
    let preferences: &[&str] = match kind {
        ProviderKind::Anthropic => &["haiku"],
        ProviderKind::OpenAi => &["gpt-4.1-mini", "gpt-4o-mini", "gpt-4.1-nano"],
        // Local servers: switching models means loading another one. Use the chat model.
        ProviderKind::LmStudio | ProviderKind::Strata => &[],
    };
    preferences.iter().find_map(|p| {
        models
            .iter()
            .find(|m| m.model.contains(p) && m.capabilities.tools)
            .map(|m| m.id.clone())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxide_core::ModelCapabilities;

    #[test]
    fn titles_are_cleaned() {
        assert_eq!(
            clean_title("\n\n\"Rust lifetimes explained.\"\nmore").as_deref(),
            Some("Rust lifetimes explained")
        );
        assert_eq!(
            clean_title("Title: **Trip to Oslo**").as_deref(),
            Some("Trip to Oslo")
        );
        assert_eq!(clean_title("# Budget plan").as_deref(), Some("Budget plan"));
        assert_eq!(clean_title("  \n \"\" "), None);
        let long = clean_title(&"word ".repeat(30)).expect("title");
        assert_eq!(long.chars().count(), MAX_TITLE_CHARS);
        assert!(long.ends_with('…'));
    }

    #[test]
    fn title_models_prefer_small_ones_on_the_same_endpoint() {
        let m = |id: &str| ModelInfo {
            id: format!("p/{id}"),
            provider_id: "p".into(),
            model: id.into(),
            label: id.into(),
            capabilities: ModelCapabilities {
                tools: true,
                ..ModelCapabilities::default()
            },
            context_window: None,
            max_output_tokens: None,
        };
        let models = vec![m("claude-opus-5-5"), m("claude-haiku-4-5")];
        assert_eq!(
            pick_title_model(ProviderKind::Anthropic, &models).as_deref(),
            Some("p/claude-haiku-4-5")
        );
        assert_eq!(pick_title_model(ProviderKind::LmStudio, &models), None);
        assert_eq!(pick_title_model(ProviderKind::OpenAi, &[m("gpt-5")]), None);
    }

    #[test]
    fn prompt_is_capped() {
        let p = title_prompt(&"a".repeat(5_000), "b");
        assert!(p.len() < 2_300);
    }
}
