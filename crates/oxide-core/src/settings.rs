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

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct AppSettings {
    pub profile: ProfileSettings,
    pub appearance: AppearanceSettings,
}

impl AppSettings {
    /// Trims free-text fields and caps their length.
    pub fn normalized(mut self) -> Self {
        self.profile.name = self.profile.name.trim().chars().take(80).collect();
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
}
