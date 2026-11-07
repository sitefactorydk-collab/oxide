//! App settings, stored as one typed JSON document.
//!
//! Every struct uses `#[serde(default)]`, so settings saved by an older build still load
//! after fields are added.

use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ThemePref {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ChatFont {
    #[default]
    Serif,
    Sans,
    System,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct ProfileSettings {
    /// What Oxide calls the user (greeting, avatar initials).
    pub name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct AppearanceSettings {
    pub theme: ThemePref,
    pub chat_font: ChatFont,
}

/// Default extended-thinking budget, in tokens (for models that take a budget).
pub const DEFAULT_THINKING_BUDGET: u32 = 8_000;
/// Smallest budget the APIs accept.
pub const MIN_THINKING_BUDGET: u32 = 1_024;
/// Largest budget Oxide asks for.
pub const MAX_THINKING_BUDGET: u32 = 64_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct ModelSettings {
    /// List models without native tool calling (greyed out, not selectable).
    pub show_unsupported: bool,
    /// Name new chats after their first exchange.
    pub auto_title: bool,
    /// Model used for automatic titles (`<providerId>/<model>`). `None` picks a small model
    /// from the same endpoint as the chat.
    pub title_model: Option<String>,
    /// Extended-thinking budget in tokens, for models that take one.
    pub thinking_budget: u32,
}

impl Default for ModelSettings {
    fn default() -> Self {
        Self {
            show_unsupported: false,
            auto_title: true,
            title_model: None,
            thinking_budget: DEFAULT_THINKING_BUDGET,
        }
    }
}

/// Default number of tool steps in one reply.
pub const DEFAULT_MAX_STEPS: u32 = 25;
/// Most tool steps Oxide allows in one reply.
pub const MAX_MAX_STEPS: u32 = 200;

/// Tool approvals and the tool loop (Settings → Permissions).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct ToolSettings {
    /// Run MCP tools that say they only read (`readOnlyHint`) without asking.
    pub auto_allow_read_only: bool,
    /// Ask before Oxide's own web search and web fetch run.
    pub ask_before_web: bool,
    /// Most model → tools → model rounds in one reply.
    pub max_steps: u32,
}

impl Default for ToolSettings {
    fn default() -> Self {
        Self {
            auto_allow_read_only: false,
            ask_before_web: false,
            max_steps: DEFAULT_MAX_STEPS,
        }
    }
}

/// Which service Oxide's `web_search` tool uses.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum WebSearchProvider {
    /// No search service: models get no `web_search` tool (except Anthropic's own, if on).
    #[default]
    None,
    Brave,
    Tavily,
    Searxng,
}

/// Web search (Settings → Web search). API keys live in the keychain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct WebSearchSettings {
    pub provider: WebSearchProvider,
    /// Base URL of a SearXNG instance (its JSON format must be enabled).
    pub searxng_url: String,
    /// On Anthropic models, use Anthropic's server-side web search instead.
    pub anthropic_native: bool,
    /// Results per search (1–10).
    pub max_results: u32,
}

impl Default for WebSearchSettings {
    fn default() -> Self {
        Self {
            provider: WebSearchProvider::None,
            searxng_url: String::new(),
            anthropic_native: true,
            max_results: 5,
        }
    }
}

/// Default knowledge budget: projects up to this size go into the prompt in full.
pub const DEFAULT_KNOWLEDGE_BUDGET: u32 = 40_000;
/// Smallest and largest knowledge budget Settings accepts.
pub const MIN_KNOWLEDGE_BUDGET: u32 = 1_000;
pub const MAX_KNOWLEDGE_BUDGET: u32 = 400_000;

/// Projects (Settings → Projects & memory).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct ProjectSettings {
    /// Estimated tokens of knowledge that go into the prompt in full. Larger projects are
    /// searched instead. A chat also keeps it under a quarter of the model's context window.
    pub knowledge_budget: u32,
}

impl Default for ProjectSettings {
    fn default() -> Self {
        Self {
            knowledge_budget: DEFAULT_KNOWLEDGE_BUDGET,
        }
    }
}

/// Memory across chats (opt-in).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct MemorySettings {
    /// Learn memory items from chats, and give saved items to models.
    pub enabled: bool,
    /// Model that writes memory items (`<providerId>/<model>`). `None` picks a small model
    /// from the chat's endpoint, like automatic titles.
    pub model: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct AppSettings {
    pub profile: ProfileSettings,
    pub appearance: AppearanceSettings,
    pub models: ModelSettings,
    pub tools: ToolSettings,
    pub web_search: WebSearchSettings,
    pub projects: ProjectSettings,
    pub memory: MemorySettings,
}

impl AppSettings {
    /// Trims free-text fields and caps their length.
    pub fn normalized(mut self) -> Self {
        self.profile.name = self.profile.name.trim().chars().take(80).collect();
        self.models.thinking_budget = self
            .models
            .thinking_budget
            .clamp(MIN_THINKING_BUDGET, MAX_THINKING_BUDGET);
        self.models.title_model = self
            .models
            .title_model
            .map(|m| m.trim().to_owned())
            .filter(|m| crate::model::split_model_id(m).is_some());
        self.tools.max_steps = self.tools.max_steps.clamp(1, MAX_MAX_STEPS);
        self.web_search.max_results = self.web_search.max_results.clamp(1, 10);
        self.projects.knowledge_budget = self
            .projects
            .knowledge_budget
            .clamp(MIN_KNOWLEDGE_BUDGET, MAX_KNOWLEDGE_BUDGET);
        self.memory.model = self
            .memory
            .model
            .map(|m| m.trim().to_owned())
            .filter(|m| crate::model::split_model_id(m).is_some());
        self.web_search.searxng_url = self
            .web_search
            .searxng_url
            .trim()
            .trim_end_matches('/')
            .to_owned();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let s: AppSettings = serde_json::from_str(r#"{"profile":{"name":"Ada"}}"#).expect("parses");
        assert_eq!(s.profile.name, "Ada");
        assert_eq!(s.appearance, AppearanceSettings::default());
    }

    #[test]
    fn unknown_fields_are_ignored() {
        let s: AppSettings =
            serde_json::from_str(r#"{"appearance":{"theme":"dark","future":1}}"#).expect("parses");
        assert_eq!(s.appearance.theme, ThemePref::Dark);
    }

    #[test]
    fn normalized_trims_name() {
        let mut s = AppSettings::default();
        s.profile.name = "  Mikkel ".into();
        assert_eq!(s.normalized().profile.name, "Mikkel");
    }

    #[test]
    fn model_settings_default_and_clamp() {
        let s: AppSettings = serde_json::from_str("{}").expect("parses");
        assert!(s.models.auto_title);
        assert_eq!(s.models.thinking_budget, DEFAULT_THINKING_BUDGET);
        let mut s = AppSettings::default();
        s.models.thinking_budget = 10;
        s.models.title_model = Some("  ".into());
        let s = s.normalized();
        assert_eq!(s.models.thinking_budget, MIN_THINKING_BUDGET);
        assert_eq!(s.models.title_model, None);
    }

    #[test]
    fn tool_settings_default_and_clamp() {
        let s: AppSettings = serde_json::from_str("{}").expect("parses");
        assert_eq!(s.tools.max_steps, DEFAULT_MAX_STEPS);
        assert!(!s.tools.auto_allow_read_only);
        assert!(s.web_search.anthropic_native);
        assert_eq!(s.web_search.provider, WebSearchProvider::None);
        let mut s = AppSettings::default();
        s.tools.max_steps = 0;
        s.web_search.max_results = 50;
        s.web_search.searxng_url = " http://localhost:8888/ ".into();
        let s = s.normalized();
        assert_eq!(s.tools.max_steps, 1);
        assert_eq!(s.web_search.max_results, 10);
        assert_eq!(s.web_search.searxng_url, "http://localhost:8888");
    }

    #[test]
    fn project_and_memory_settings() {
        let s: AppSettings = serde_json::from_str("{}").expect("parses");
        assert_eq!(s.projects.knowledge_budget, DEFAULT_KNOWLEDGE_BUDGET);
        assert!(!s.memory.enabled, "memory is opt-in");
        let mut s = AppSettings::default();
        s.projects.knowledge_budget = 5;
        s.memory.model = Some("nope".into());
        let s = s.normalized();
        assert_eq!(s.projects.knowledge_budget, MIN_KNOWLEDGE_BUDGET);
        assert_eq!(s.memory.model, None);
    }
}
