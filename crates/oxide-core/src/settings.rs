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

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct AppSettings {
    pub profile: ProfileSettings,
    pub appearance: AppearanceSettings,
    pub models: ModelSettings,
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
}
